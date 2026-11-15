use super::*;

fn config(model: &str) -> ProviderConfig {
    ProviderConfig {
        model: model.into(),
        base_url: Some("http://example.invalid".into()),
        api_key: "synthetic-key".into(),
        temperature: None,
        max_tokens: Some(120),
        reasoning: None,
        verbosity: None,
        pro_mode: false,
    }
}

#[test]
fn syc_free_caps_each_request_and_preserves_the_filter_question() {
    for model in ["openrouter/free", "vendor/model:free"] {
        let body = free_body(
            &config(model),
            "Return a filter.",
            &[Message::user_text("Show scores above 8")],
            &[],
        )
        .unwrap();
        assert_eq!(body["model"], model);
        assert_eq!(body["reasoning"]["effort"], "none");
        assert_eq!(
            body["provider"]["max_price"],
            json!({"prompt":0,"completion":0,"request":0,"image":0})
        );
        assert_eq!(body["messages"][1]["content"], "Show scores above 8");
        assert!(!body.to_string().contains("synthetic-key"));
    }
}

#[test]
fn syc_free_refuses_paid_models_and_paid_plugin_suffixes() {
    for model in [
        "vendor/paid",
        "vendor/model:online:free",
        "openrouter/auto",
        ":free",
    ] {
        assert!(free_body(&config(model), "", &[], &[]).is_err());
    }
}

#[test]
#[ignore = "uses one free OpenRouter request and requires an OpenRouter login"]
fn syc_free_live_filter_on_a_synthetic_column() {
    use crate::app::chat::{ask_filter, providers};
    use octa::data::{ColumnInfo, conditional_format::CondOp};
    use octa::ui::settings::{AppSettings, ChatProviderKind, secrets};
    let kind = ChatProviderKind::SycFree;
    let mut cfg = config(kind.default_model());
    cfg.api_key = secrets::get_api_key(kind, &AppSettings::default()).expect("OpenRouter login");
    cfg.max_tokens = Some(1024);
    let columns = [ColumnInfo {
        name: "score".into(),
        data_type: "Int64".into(),
    }];
    let result = ask_filter::ask(
        providers::make_provider(kind).as_ref(),
        &cfg,
        &columns,
        3,
        "Show rows where score is greater than 8.",
        &AtomicBool::new(false),
        &mut (0, 0),
    )
    .expect("live filter");
    assert_eq!(result.predicates.len(), 1);
    assert_eq!(result.predicates[0].col, 0);
    assert_eq!(result.predicates[0].op, CondOp::Gt);
    assert_eq!(result.predicates[0].value, "8");
}
