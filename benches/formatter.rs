use anyhow::{Context, Result};
use cpu_time::ProcessTime;
use criterion::{BenchmarkId, Criterion, Throughput};
use ijfmt::{config, formatter};
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Duration;

fn workloads() -> Result<Vec<(String, String)>> {
    let mut workloads = vec![
        ("small".into(), "class Small{int value=1;}\n".into()),
        (
            "deep".into(),
            format!(
                "class Deep {{ int value = {}1{}; }}\n",
                "(".repeat(128),
                ")".repeat(128),
            ),
        ),
        (
            "chains".into(),
            format!(
                "class Chains {{ void run() {{ {} }} }}\n",
                "service.first(\"a long argument to exercise wrapping\").second(1, 2, 3).third(value -> transform(value));\n".repeat(64),
            ),
        ),
        (
            "comments".into(),
            format!(
                "class Comments {{ {} }}\n",
                (0..64)
                    .map(|i| format!(
                        "/** A method with a description that needs wrapping and several documented arguments.\n * @param value the value to transform\n * @return the transformed value\n */\nint method{i}(int value){{return value+1;}}\n",
                    ))
                    .collect::<String>(),
            ),
        ),
        (
            "large".into(),
            format!(
                "class Large {{ {} }}\n",
                (0..256)
                    .map(|i| format!(
                        "int method{i}(int value){{if(value>0){{return value*2;}}else{{return -value;}}}}\n",
                    ))
                    .collect::<String>(),
            ),
        ),
    ];

    if let Some(directory) = std::env::var_os("IJFMT_BENCH_DIR") {
        let mut paths = fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.retain(|path| {
            path.extension()
                .is_some_and(|extension| extension == "java")
        });
        paths.sort();
        anyhow::ensure!(!paths.is_empty(), "IJFMT_BENCH_DIR contains no Java files");
        for path in paths {
            let bytes = fs::read(&path)
                .with_context(|| format!("Cannot read benchmark input {}", path.display()))?;
            workloads.push((
                path.display().to_string(),
                String::from_utf8_lossy(&bytes).into_owned(),
            ));
        }
    }

    Ok(workloads)
}

fn run() -> Result<()> {
    let cpu_time = match std::env::var("IJFMT_BENCH_CLOCK").as_deref() {
        Ok("cpu") | Err(std::env::VarError::NotPresent) => true,
        Ok("wall") => false,
        _ => anyhow::bail!("IJFMT_BENCH_CLOCK must be cpu or wall"),
    };
    if cpu_time {
        ProcessTime::try_now().context("CPU clock unavailable; set IJFMT_BENCH_CLOCK=wall")?;
    }
    let mut criterion = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .configure_from_args();
    let style_path = std::env::var_os("IJFMT_BENCH_STYLE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fmt/Sample.xml"));
    let xml = fs::read_to_string(&style_path)
        .with_context(|| format!("Cannot read benchmark style {}", style_path.display()))?;
    let config = config::parse_style_xml(&xml)?;

    let mut group = criterion.benchmark_group(if cpu_time {
        "format_process_cpu"
    } else {
        "format_wall"
    });
    for (name, source) in workloads()? {
        formatter::format(&source, &config)
            .with_context(|| format!("Cannot format benchmark input {name}"))?;
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            &source,
            |bencher, source| {
                let format =
                    || black_box(formatter::format(black_box(source), black_box(&config)).unwrap());
                if cpu_time {
                    bencher.iter_custom(|iterations| {
                        let start = ProcessTime::now();
                        for _ in 0..iterations {
                            format();
                        }
                        start.elapsed()
                    });
                } else {
                    bencher.iter(format);
                }
            },
        );
    }
    group.finish();
    criterion.final_summary();
    Ok(())
}

fn main() -> Result<()> {
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(run)?
        .join()
        .map_err(|_| anyhow::anyhow!("Benchmark thread panicked"))?
}
