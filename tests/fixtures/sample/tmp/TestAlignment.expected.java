class Test {
    void method() {
        final Schema<?> nestedArraySchema = new ArraySchema().items(new StringSchema());
        final Schema<?> mapSchema = new MapSchema().additionalProperties(new Schema().$ref("#/components/schemas/NestedArray"));
    }
}
