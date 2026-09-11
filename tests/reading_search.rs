#[path = "../vendor/gpui-component/src/text/source_map.rs"]
mod source_map;

#[cfg(feature = "editor-diagnostics")]
#[test]
fn reading_renderer_maps_actual_markdown() {
    gpui_component::text::TextView::diagnostic_reading_search();
}
