# The HUB75 driver dependency is declared in idf_component.yml; the ESP-IDF
# component manager fetches it and auto-adds it to this component's REQUIRES,
# so matrix_shim.cpp can #include the library header without listing it here.
idf_component_register(
    SRCS "matrix_shim.cpp"
    INCLUDE_DIRS "."
)
