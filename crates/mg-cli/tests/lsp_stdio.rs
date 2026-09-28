//! `mg lsp` over real stdio (plan 4, L0): the built binary, spoken to
//! with `Content-Length`-framed JSON-RPC exactly as an editor would.

use std::io::BufReader;
use std::process::{Command, Stdio};

use lsp_server::{Message, Notification, Request, RequestId};
use serde_json::json;

#[test]
fn mg_lsp_serves_a_session_over_stdio() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mg"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("mg starts");
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut send = |message: Message| message.write(&mut stdin).unwrap();

    send(
        Request::new(
            RequestId::from(1),
            "initialize".into(),
            json!({ "capabilities": {} }),
        )
        .into(),
    );
    let Some(Message::Response(response)) = Message::read(&mut stdout).unwrap() else {
        panic!("expected the initialize response");
    };
    let result = response.response_result.unwrap();
    assert_eq!(result["capabilities"]["positionEncoding"], "utf-16");
    send(Notification::new("initialized".into(), json!({})).into());

    send(
        Notification::new(
            "textDocument/didOpen".into(),
            json!({ "textDocument": {
                "uri": "file:///broken.mg",
                "languageId": "metaglyph",
                "version": 1,
                "text": "glyph A (advance: 1 {}\n",
            }}),
        )
        .into(),
    );
    let Some(Message::Notification(published)) = Message::read(&mut stdout).unwrap() else {
        panic!("expected publishDiagnostics");
    };
    assert_eq!(published.method, "textDocument/publishDiagnostics");
    let diagnostics = published.params["diagnostics"].as_array().unwrap();
    assert!(!diagnostics.is_empty());
    assert!(diagnostics[0]["code"].as_str().unwrap().starts_with("MG01"));

    send(Request::new(RequestId::from(2), "shutdown".into(), json!(null)).into());
    let Some(Message::Response(response)) = Message::read(&mut stdout).unwrap() else {
        panic!("expected the shutdown response");
    };
    assert!(response.response_result.is_ok());
    send(Notification::new("exit".into(), json!(null)).into());
    drop(stdin);

    let status = child.wait().unwrap();
    assert!(status.success(), "mg lsp exits cleanly: {status}");
}
