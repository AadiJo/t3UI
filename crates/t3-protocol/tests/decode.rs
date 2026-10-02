//! Decoding and encoding edge cases of the wire types.
//!
//! Failure modes this guards against:
//! 1. A numeric `requestId` (rc.115 echoes the JSON type it got) fails to decode or is not
//!    normalized to the string form used for routing.
//! 2. A batch frame (JSON array of messages) is rejected.
//! 3. An unknown frame `_tag`, stream item `kind`, or event `type` fails the decode instead of
//!    becoming `Unknown`; an unknown event loses its envelope `sequence`.
//! 4. A new literal value (session status, runtime mode, editor) fails the decode.
//! 5. Unknown fields anywhere fail the decode.
//! 6. A legacy `ModelSelection` (`provider` key, object-form `options`) fails or drops options,
//!    or encoding writes anything but the array form.
//! 7. `optional(NullOr)` patch fields cannot tell "absent" (no change) from `null` (clear).
//! 8. One undecodable element of a forward-compatible array (providers, keybindings) drops the
//!    whole config.
//! 9. Commands serialize optional keys as `null` (the server rejects `null` for optionalKey),
//!    use the wrong `type` string, or the wrong field casing.
//! 10. Typed errors lose the fields the UI needs (`requiredScope`, `bootstrapThreadDisposition`).
//! 11. A provider that omits `supportsConversationRollback`, `supportsTextGeneration`, or
//!     `showInteractionModeToggle` decodes as unsupported. The web client treats absent as
//!     supported (`!== false`), so "Edit from here" and the mode toggle would vanish.

use serde_json::{Value, json};
use t3_protocol::{
    CommandId, MessageId, ProjectId, ThreadId,
    commands::{ClientCommand, ThreadMetaPatch, TurnStart, TurnStartMessage, UserRole},
    errors::ServerError,
    orchestration::{
        EventBody, InteractionMode, ModelSelection, OrchestrationEvent, ProviderOptionValue,
        RuntimeMode, SessionStatus, ShellStreamItem, ThreadStreamItem,
    },
    rpc::{CauseReason, ClientFrame, ExitEncoded, ServerFrame},
    server::{EditorId, ServerConfigStreamEvent},
};

fn event(event_type: &str, payload: Value) -> Value {
    json!({
        "sequence": 42,
        "eventId": "e1",
        "aggregateKind": "thread",
        "aggregateId": "t1",
        "occurredAt": "2026-10-01T00:00:00.000Z",
        "commandId": null,
        "causationEventId": null,
        "correlationId": null,
        "metadata": {},
        "type": event_type,
        "payload": payload,
        "someFutureEnvelopeField": true,
    })
}

#[test]
fn numeric_and_string_request_ids_normalize() {
    let frame = ServerFrame::decode(
        r#"{"_tag":"Exit","requestId":4,"exit":{"_tag":"Success","value":null}}"#,
    )
    .unwrap();
    assert!(matches!(frame, ServerFrame::Exit { ref request_id, .. } if request_id == "4"));

    let frame =
        ServerFrame::decode(r#"{"_tag":"Chunk","requestId":"7","values":[{"a":1}]}"#).unwrap();
    assert!(
        matches!(frame, ServerFrame::Chunk { ref request_id, ref values } if request_id == "7" && values.len() == 1)
    );
}

#[test]
fn batch_frames_and_unknown_tags_decode() {
    let frames = ServerFrame::decode_all(
        r#"[{"_tag":"Pong"},{"_tag":"Exit","requestId":"5","exit":{"_tag":"Success","value":"ok"}},{"_tag":"Bogus"}]"#,
    )
    .unwrap();
    assert_eq!(frames.len(), 3);
    assert!(matches!(frames[0], ServerFrame::Pong));
    assert!(matches!(frames[2], ServerFrame::Unknown { ref tag } if tag == "Bogus"));
}

#[test]
fn exit_failures_keep_typed_errors_and_defects() {
    let frame = ServerFrame::decode(
        r#"{"_tag":"Exit","requestId":"2","exit":{"_tag":"Failure","cause":[{"_tag":"Fail","error":{"_tag":"OrchestrationDispatchCommandError","message":"nope","bootstrapThreadDisposition":"not-created"}},{"_tag":"Die","defect":"Unknown request tag: demo.nope"},{"_tag":"Interrupt","fiberId":null}]}}"#,
    )
    .unwrap();
    let ServerFrame::Exit {
        exit: ExitEncoded::Failure { cause },
        ..
    } = frame
    else {
        panic!("expected a failure exit");
    };
    let [
        CauseReason::Fail { error },
        CauseReason::Die { defect },
        CauseReason::Interrupt,
    ] = cause.as_slice()
    else {
        panic!("unexpected causes: {cause:?}");
    };
    let error: ServerError = serde_json::from_str(error.get()).unwrap();
    assert_eq!(error.tag, ServerError::DISPATCH);
    assert_eq!(error.display_message(), "nope");
    assert_eq!(error.bootstrap_thread_disposition(), Some("not-created"));
    assert_eq!(defect, "Unknown request tag: demo.nope");

    let auth: ServerError = serde_json::from_value(json!({
        "_tag": "EnvironmentAuthorizationError",
        "message": "missing scope",
        "requiredScope": "access:read",
    }))
    .unwrap();
    assert!(auth.is_authorization());
    assert_eq!(auth.required_scope.as_deref(), Some("access:read"));
}

#[test]
fn client_frames_match_effect_encoding() {
    let request = ClientFrame::Request {
        id: "1".into(),
        tag: "server.getConfig".into(),
        payload: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        headers: vec![],
    };
    assert_eq!(
        serde_json::to_string(&request).unwrap(),
        r#"{"_tag":"Request","id":"1","tag":"server.getConfig","payload":{},"headers":[]}"#
    );
    assert_eq!(
        serde_json::to_string(&ClientFrame::Ack {
            request_id: "7".into()
        })
        .unwrap(),
        r#"{"_tag":"Ack","requestId":"7"}"#
    );
}

#[test]
fn unknown_stream_kinds_and_event_types_are_tolerated() {
    let item: ShellStreamItem =
        serde_json::from_value(json!({"kind": "project-renamed-v2"})).unwrap();
    assert_eq!(
        item,
        ShellStreamItem::Unknown {
            kind: "project-renamed-v2".into()
        }
    );

    let item: ThreadStreamItem = serde_json::from_value(json!({
        "kind": "event",
        "event": event("thread.teleported", json!({"threadId": "t1", "to": "mars"})),
    }))
    .unwrap();
    let ThreadStreamItem::Event(event) = item else {
        panic!("expected an event item");
    };
    assert_eq!(event.sequence, 42);
    assert!(
        matches!(&event.body, EventBody::Unknown { event_type, payload } if event_type == "thread.teleported" && payload["to"] == "mars")
    );

    let config: ServerConfigStreamEvent =
        serde_json::from_value(json!({"version": 1, "type": "somethingNew", "payload": {}}))
            .unwrap();
    assert_eq!(config, ServerConfigStreamEvent::Unknown);
}

#[test]
fn unknown_literals_become_other() {
    let status: SessionStatus = serde_json::from_value(json!("hibernating")).unwrap();
    assert_eq!(status, SessionStatus::Other("hibernating".into()));
    assert_eq!(serde_json::to_value(&status).unwrap(), json!("hibernating"));
    assert_eq!(
        serde_json::from_value::<RuntimeMode>(json!("full-access")).unwrap(),
        RuntimeMode::FullAccess
    );
    assert_eq!(
        serde_json::from_value::<EditorId>(json!("vscode-insiders")).unwrap(),
        EditorId::VscodeInsiders
    );
}

#[test]
fn message_event_with_unknown_fields_decodes() {
    let event: OrchestrationEvent = serde_json::from_value(event(
        "thread.message-sent",
        json!({
            "threadId": "t1",
            "messageId": "assistant:1",
            "role": "assistant",
            "text": "Hel",
            "turnId": "turn-1",
            "streaming": true,
            "createdAt": "2026-10-01T00:00:00.000Z",
            "updatedAt": "2026-10-01T00:00:00.000Z",
            "tokensSoFar": 3,
        }),
    ))
    .unwrap();
    let EventBody::ThreadMessageSent(payload) = event.body else {
        panic!("expected message-sent");
    };
    assert_eq!(payload.text, "Hel");
    assert!(payload.streaming);
    assert_eq!(payload.message_id, MessageId::from("assistant:1"));
}

#[test]
fn legacy_model_selection_decodes_and_encodes_canonically() {
    let legacy: ModelSelection = serde_json::from_value(json!({
        "provider": "codex",
        "model": "gpt-5.5",
        "options": {"effort": " high ", "fastMode": true, "weird": 3, " ": "x"},
    }))
    .unwrap();
    assert_eq!(legacy.instance_id.as_str(), "codex");
    let mut options: Vec<_> = legacy
        .options
        .iter()
        .map(|o| (o.id.as_str(), o.value.clone()))
        .collect();
    options.sort_by_key(|(id, _)| *id);
    assert_eq!(
        options,
        vec![
            ("effort", ProviderOptionValue::String("high".into())),
            ("fastMode", ProviderOptionValue::Bool(true)),
        ]
    );
    let encoded = serde_json::to_value(&legacy).unwrap();
    assert_eq!(encoded["instanceId"], "codex");
    assert!(encoded.get("provider").is_none());
    assert!(encoded["options"].is_array());

    let bare: ModelSelection =
        serde_json::from_value(json!({"instanceId": "claudeAgent", "model": "opus"})).unwrap();
    assert!(
        serde_json::to_value(&bare)
            .unwrap()
            .get("options")
            .is_none()
    );
}

#[test]
fn meta_update_distinguishes_absent_from_null() {
    let decode = |payload: Value| {
        let event: OrchestrationEvent =
            serde_json::from_value(event("thread.meta-updated", payload)).unwrap();
        match event.body {
            EventBody::ThreadMetaUpdated(payload) => payload,
            other => panic!("unexpected {other:?}"),
        }
    };
    let cleared = decode(json!({"threadId": "t1", "branch": null, "updatedAt": "x"}));
    assert_eq!(cleared.branch, Some(None));
    assert_eq!(cleared.worktree_path, None);
    let set = decode(json!({"threadId": "t1", "branch": "main", "updatedAt": "x"}));
    assert_eq!(set.branch, Some(Some("main".into())));
}

#[test]
fn forward_compatible_arrays_skip_bad_elements() {
    #[derive(serde::Deserialize)]
    struct Providers {
        #[serde(deserialize_with = "t3_protocol::schema::forward_compatible")]
        providers: Vec<t3_protocol::server::ServerProvider>,
    }
    let decoded: Providers = serde_json::from_value(json!({
        "providers": [
            {"instanceId": "codex", "driver": "codex", "enabled": true, "installed": true,
             "version": "1", "status": "ready", "auth": {"status": "authenticated"},
             "checkedAt": "x", "models": [], "slashCommands": [], "skills": []},
            {"instanceId": 12},
        ]
    }))
    .unwrap();
    assert_eq!(decoded.providers.len(), 1);
}

#[test]
fn commands_omit_absent_optionals_and_use_wire_names() {
    let archive = ClientCommand::ThreadArchive {
        command_id: CommandId::from("c1"),
        thread_id: ThreadId::from("t1"),
    };
    assert_eq!(
        serde_json::to_value(&archive).unwrap(),
        json!({"type": "thread.archive", "commandId": "c1", "threadId": "t1"})
    );

    let rename = ClientCommand::ThreadMetaUpdate {
        command_id: CommandId::from("c2"),
        thread_id: ThreadId::from("t1"),
        patch: ThreadMetaPatch {
            title: Some("New".into()),
            branch: Some(None),
            ..Default::default()
        },
    };
    assert_eq!(
        serde_json::to_value(&rename).unwrap(),
        json!({"type": "thread.meta.update", "commandId": "c2", "threadId": "t1", "title": "New", "branch": null})
    );

    let create = ClientCommand::ThreadCreate {
        command_id: CommandId::from("c3"),
        thread_id: ThreadId::from("t2"),
        project_id: ProjectId::from("p1"),
        title: "T".into(),
        model_selection: ModelSelection {
            instance_id: "codex".into(),
            model: "gpt-5.5".into(),
            options: vec![],
        },
        runtime_mode: RuntimeMode::FullAccess,
        interaction_mode: Some(InteractionMode::Default),
        branch: None,
        worktree_path: None,
        created_at: "x".into(),
    };
    let value = serde_json::to_value(&create).unwrap();
    // `NullOr` keys are required and written as null.
    assert_eq!(value["branch"], Value::Null);
    assert!(value.as_object().unwrap().contains_key("worktreePath"));

    let turn: ClientCommand = TurnStart {
        command_id: CommandId::from("c4"),
        thread_id: ThreadId::from("t2"),
        message: TurnStartMessage {
            message_id: MessageId::from("m1"),
            role: UserRole::User,
            text: "hi".into(),
            attachments: vec![],
            context: None,
        },
        model_selection: None,
        title_seed: None,
        runtime_mode: RuntimeMode::FullAccess,
        interaction_mode: InteractionMode::Default,
        bootstrap: None,
        source_proposed_plan: None,
        created_at: "x".into(),
    }
    .into();
    assert_eq!(
        serde_json::to_value(&turn).unwrap(),
        json!({
            "type": "thread.turn.start", "commandId": "c4", "threadId": "t2",
            "message": {"messageId": "m1", "role": "user", "text": "hi", "attachments": []},
            "runtimeMode": "full-access", "interactionMode": "default", "createdAt": "x",
        })
    );
}

#[test]
fn optional_provider_capabilities_default_to_supported() {
    let base = json!({
        "instanceId": "codex", "driver": "codex", "enabled": true, "installed": true,
        "version": "1", "status": "ready", "auth": {"status": "authenticated"},
        "checkedAt": "x", "models": [], "slashCommands": [], "skills": [],
    });
    let absent: t3_protocol::server::ServerProvider = serde_json::from_value(base.clone()).unwrap();
    assert!(absent.supports_conversation_rollback);
    assert!(absent.supports_text_generation);
    assert!(absent.show_interaction_mode_toggle);
    // These two are opt-in (`=== true` / `!== true` in the web client).
    assert!(!absent.reports_context_window);
    assert!(!absent.requires_new_thread_for_model_change);

    let mut explicit = base;
    for key in [
        "supportsConversationRollback",
        "supportsTextGeneration",
        "showInteractionModeToggle",
    ] {
        explicit[key] = json!(false);
    }
    let explicit: t3_protocol::server::ServerProvider = serde_json::from_value(explicit).unwrap();
    assert!(!explicit.supports_conversation_rollback);
    assert!(!explicit.supports_text_generation);
    assert!(!explicit.show_interaction_mode_toggle);
}
