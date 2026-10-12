package com.example.db;

import static com.example.db.Queries.INSERT_RECORD;

import com.example.db.exception.DatabaseException;
import io.example.pool.ConnectionPool;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.stereotype.Repository;

import java.sql.Connection;
import java.sql.PreparedStatement;
import java.sql.SQLException;
import java.util.Date;
import java.util.UUID;

/*
 * Generic database repository used as a public fixture for ijfmt tests.
 * This file uses the Sample style: tabs, Allman braces, aligned fields.
 */
@Repository
public class DatabaseRepository
{
	private static final Logger         LOGGER     = LoggerFactory.getLogger(DatabaseRepository.class);
	private static final String         TABLE_KEY  = "records";
	private static final String         TABLE_NAME = "db_records";
	private static final int            MAX_RETRY  = 3;
	private final        ConnectionPool pool;

	@Autowired
	public DatabaseRepository(ConnectionPool pool)
	{
		this.pool = pool;

		initSchema();
	}

	private void initSchema() throws DatabaseException
	{
		try (Connection con = pool.getConnection())
		{
			String table    = pool.getTableName(TABLE_KEY, TABLE_NAME);
			String logTable = pool.getLogTableName(TABLE_KEY, TABLE_NAME);

			con.prepareStatement(
					"CREATE TABLE IF NOT EXISTS `" + table + "` (" +
							"`id` VARCHAR(64) PRIMARY KEY," +
							"`date` DATETIME DEFAULT NULL," +
							"`uuid` CHAR(36) NOT NULL," +
							"`status` VARCHAR(64) DEFAULT NULL," +
							"`amount` DOUBLE DEFAULT NULL," +
							"`currency` CHAR(3) DEFAULT NULL" +
							");"
			).execute();

			con.prepareStatement(
					"CREATE TABLE IF NOT EXISTS `" + logTable + "` (" +
							"`id` VARCHAR(64) PRIMARY KEY," +
							"`date` DATETIME DEFAULT NULL," +
							"`uuid` CHAR(36) NOT NULL," +
							"`status` VARCHAR(64) DEFAULT NULL," +
							"`amount` DOUBLE DEFAULT NULL," +
							"`currency` CHAR(3) DEFAULT NULL" +
							");"
			).execute();
		} catch (SQLException e)
		{
			LOGGER.error("Failed to init schema", e);
			throw new DatabaseException("Schema init failed", e);
		}
	}

	public void insertRecord(String id, Date date, UUID uuid, String status, Double amount, String currency) throws DatabaseException
	{
		try (Connection con = pool.getConnection())
		{
			String table = pool.getTableName(TABLE_KEY, TABLE_NAME);
			con.prepareStatement(INSERT_RECORD + table + "` (`id`, `date`, `uuid`, `status`, `amount`, `currency`) VALUES (?, ?, ?, ?, ?, ?);")
					.execute();
		} catch (SQLException e)
		{
			LOGGER.error("Failed to insert record", e);
			throw new DatabaseException("Insert failed", e);
		}
	}

	public void updateStatus(String id, String status) throws DatabaseException
	{
		try (Connection con = pool.getConnection())
		{
			String table = pool.getTableName(TABLE_KEY, TABLE_NAME);

			con.prepareStatement("UPDATE `" + table + "` SET status=? WHERE id=?;")
					.execute();
		} catch (SQLException e)
		{
			LOGGER.error("Failed to update status", e);
			throw new DatabaseException("Update failed", e);
		}
	}

	public void insertLog(String id, Date date, UUID uuid, String status, Double amount, String currency)
	{
		try (Connection con = pool.getConnection())
		{
			String table = pool.getLogTableName(TABLE_KEY, TABLE_NAME);

			con.prepareStatement(INSERT_RECORD + table + "` (`id`, `date`, `uuid`, `status`, `amount`, `currency`) VALUES (?, ?, ?, ?, ?, ?);")
					.execute();
		} catch (SQLException e)
		{
			LOGGER.error("Failed to insert log", e);
		}
	}
}
