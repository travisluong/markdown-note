use merman::{Engine, ParseOptions};
use serde_json::{Value, json};

// Keep the upstream sequence suite running against the patched dependency in CI.
#[path = "../vendor/merman-core/src/tests/sequence.rs"]
mod upstream;

const SPACED_SEQUENCE: &str =
    include_str!("../vendor/gpui-component/src/mermaid/spaced-sequence.mmd");

fn parse(source: &str) -> Value {
    Engine::new()
        .parse_diagram_sync(source, ParseOptions::default())
        .unwrap_or_else(|e| panic!("{source}: {e}"))
        .unwrap()
        .model
}

#[test]
fn spaced_participants_preserve_identity_order_and_messages() {
    for source in [
        SPACED_SEQUENCE.to_owned(),
        SPACED_SEQUENCE.replace('\n', "\r\n"),
    ] {
        let model = parse(&source);
        let names = [
            "viewer",
            "blue node",
            "blue worker",
            "green node",
            "green worker",
        ];
        assert_eq!(model["actorOrder"], json!(names));
        assert_eq!(model["actors"].as_object().unwrap().len(), names.len());
        for name in names {
            assert_eq!(model["actors"][name]["description"], name);
        }
        let messages = model["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 17);
        for (index, message) in messages.iter().enumerate() {
            assert_eq!(message["message"], format!("Step {:02}", index + 1));
            assert!(names.iter().any(|name| message["from"] == *name));
            assert!(names.iter().any(|name| message["to"] == *name));
        }
        assert_eq!(messages[1]["from"], "blue node");
        assert_eq!(messages[1]["to"], "blue worker");
        assert_eq!(messages[1]["type"], 1);
    }
}

#[test]
fn declarations_aliases_notes_and_activations_share_spaced_ids() {
    let model = parse(
        "sequenceDiagram\nparticipant blue node\nactor green worker\nparticipant V as Visible Label\nblue node->>+green worker: Begin\nNote over blue node,green worker: Shared note\ngreen worker-->>-blue node: Finish\nactivate blue node\ndeactivate blue node\nV->>blue node: Next",
    );
    assert_eq!(
        model["actorOrder"],
        json!(["blue node", "green worker", "V"])
    );
    assert_eq!(model["actors"]["V"]["description"], "Visible Label");
    assert_eq!(model["actors"]["green worker"]["type"], "actor");
    let messages = model["messages"].as_array().unwrap();
    let note = messages
        .iter()
        .find(|m| m["message"] == "Shared note")
        .unwrap();
    assert_eq!(note["from"], "blue node");
    assert_eq!(note["to"], "green worker");
}

#[test]
fn whitespace_unicode_numbers_and_alias_word_in_names() {
    for (from, to) in [
        ("blue\tnode", "緑 ノード"),
        ("123 node", "456 worker"),
        ("blue as node", "green as worker"),
        ("loop node", "end worker"),
        ("1", "2"),
    ] {
        let model = parse(&format!(
            "sequenceDiagram\n  \u{2003}{from}  ->>  \u{2003}{to}  : Hello\n  {to} -->> {from}: Back"
        ));
        assert_eq!(model["actorOrder"], json!([from, to]));
        assert_eq!(model["messages"][0]["from"], from);
        assert_eq!(model["messages"][0]["to"], to);
        assert_eq!(model["messages"][1]["to"], from);
    }
}

#[test]
fn spaced_names_preserve_declaration_and_statement_boundaries() {
    let model = parse(
        "sequenceDiagram\nparticipant A as Display Name\nparticipant blue node %% comment\nparticipant green worker # comment\nblue node->>green worker: Hello;green worker-->>A: Back\n",
    );
    assert_eq!(
        model["actorOrder"],
        json!(["A", "blue node", "green worker"])
    );
    assert_eq!(model["messages"].as_array().unwrap().len(), 2);
    assert_eq!(model["actors"]["A"]["description"], "Display Name");
    let model = parse(
        "sequenceDiagram\nparticipant blue node@{type: 'database'}\nblue node () ->>green worker: Hello\n",
    );
    assert_eq!(model["actorOrder"], json!(["blue node", "green worker"]));
    assert_eq!(model["actors"]["blue node"]["type"], "database");
    for source in [
        "sequenceDiagram\nblue node->>green worker",
        "sequenceDiagram\nblue node->>: Missing recipient",
        "sequenceDiagram\nparticipant A @{type: 'database'}",
    ] {
        assert!(
            Engine::new()
                .parse_diagram_sync(source, ParseOptions::default())
                .is_err(),
            "{source}"
        );
    }
}
