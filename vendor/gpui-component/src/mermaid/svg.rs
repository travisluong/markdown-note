//! Compatibility corrections for the pinned Merman 0.7.0 SVG output.
use regex::{Captures, Regex};
use std::{borrow::Cow, sync::OnceLock};

fn edge_labels() -> &'static Regex {
    static LABELS: OnceLock<Regex> = OnceLock::new();
    LABELS.get_or_init(|| {
        // Match only Merman's non-HTML flowchart edge labels, not node labels or
        // other diagram families. Text and all its rows are centered at x=0.
        Regex::new(concat!(
            r#"(<g class="edgeLabel" transform="translate\([^"]+\)"><g class="label" data-id="[^"]*" transform="translate\()"#,
            r#"[^,]+(,[^"]+\)"><g><rect class="background" x=")-2(" y="[^"]+" width=")"#,
            r#"([0-9.]+)(" height="[^"]+"/><text[^>]*text-anchor="middle">)"#,
        ))
        .expect("static edge label pattern")
    })
}

pub(super) fn align_flowchart_edge_labels(svg: &str) -> Cow<'_, str> {
    edge_labels().replace_all(svg, |label: &Captures<'_>| {
        let width: f64 = label[4].parse().expect("Merman numeric label width");
        // Upstream shifts the group left by half its width, but emits centered
        // text beside a left-aligned rectangle. Center both on the edge anchor.
        format!(
            "{}0{}{}{}{}{}",
            &label[1],
            &label[2],
            -width / 2.,
            &label[3],
            &label[4],
            &label[5]
        )
    })
}

#[cfg(feature = "editor-diagnostics")]
pub(super) fn diagnostic_checks() {
    use merman::render::{HeadlessRenderer, SvgPipeline};

    let labels = Regex::new(concat!(
        r#"<g class="edgeLabel" transform="translate\([^"]+\)"><g class="label" data-id="[^"]*" transform="translate\(([^,]+),[^"]+\)">"#,
        r#"<g><rect class="background" x="([^"]+)" y="[^"]+" width="([^"]+)" height="[^"]+"/><text[^>]*text-anchor="middle">"#,
    )).unwrap();
    for dark in [false, true] {
        for direction in ["TD", "LR"] {
            let source = format!(
                "flowchart {direction}\nA{{Query?}} -->|Yes| B[Recent]\nA -->|No| C[Search]\nC -->|A longer label| D[Results]\nD -->|\"First line<br/>Second line\"| E[Done]\nE --> A"
            );
            let original = HeadlessRenderer::new()
                .with_site_config(merman::MermaidConfig::from_value(serde_json::json!({
                    "theme": if dark { "dark" } else { "default" },
                    "htmlLabels": false, "fontFamily": "Arial"
                })))
                .render_svg_with_pipeline_sync(&source, &SvgPipeline::resvg_safe())
                .unwrap()
                .unwrap();
            let aligned = align_flowchart_edge_labels(&original);
            let mut count = 0;
            for label in labels.captures_iter(&aligned) {
                let group_x: f64 = label[1].parse().unwrap();
                let background_x: f64 = label[2].parse().unwrap();
                let width: f64 = label[3].parse().unwrap();
                assert_eq!(group_x, 0., "label must remain on the edge anchor");
                assert!(
                    (background_x + width / 2.).abs() < 0.001,
                    "background must be centered around the text origin"
                );
                count += 1;
            }
            assert_eq!(count, 4, "must cover every nonempty edge label");
            assert_eq!(align_flowchart_edge_labels(&aligned), aligned);
        }
    }
}
