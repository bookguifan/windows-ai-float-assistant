//! Real Tauri manager and configuration conversion, with MockRuntime only.
//! These tests never run the native application or create HWND/WebView2 instances.
use tauri::test::{mock_context, noop_assets, MockRuntime};

#[cfg(feature = "beta2-isolation")]
#[test]
#[allow(deprecated)] // One deterministic mock setup iteration, not a native event loop.
fn configured_ball_is_deferred_before_default_identifier_mkdir() {
    use deepseek_float_lib::isolation;
    use tauri::Manager;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let parent =
        std::env::temp_dir().join(format!("beta2-bootstrap-{}-{unique}", std::process::id()));
    std::fs::create_dir(&parent).unwrap();
    let root = parent.join("isolated");
    let forbidden = parent.join("forbidden-default-identifier");
    let identifier = "com.deepseekfloat.beta2.isolated.bootstrap-test";
    isolation::initialize(
        [
            "test".into(),
            "--beta2-isolated-data-root".into(),
            root.clone().into_os_string(),
        ],
        identifier,
    )
    .unwrap();
    let mut context = mock_context::<MockRuntime, _>(noop_assets());
    context.config_mut().identifier = identifier.into();
    context.config_mut().app.windows.push(
        serde_json::from_value(serde_json::json!({
            "label":"ball", "url":"poc/ball.html", "visible":false
        }))
        .unwrap(),
    );
    context.config_mut().app.windows.push(
        serde_json::from_value(serde_json::json!({
            "label":"not-auto-created", "url":"poc/settings.html", "create":false
        }))
        .unwrap(),
    );
    let deferred = isolation::configure_context(&mut context).unwrap();
    assert!(!context.config().app.windows[0].create,
        "Tauri automatic from_config creation drops data_directory and can mkdir LocalData/identifier");
    assert_eq!(
        deferred.len(),
        1,
        "do not enable deliberately deferred windows"
    );
    assert_eq!(deferred[0].label, "ball");
    assert!(!deferred[0].visible);
    assert_eq!(
        deferred[0].data_directory.as_ref(),
        Some(&root.join("webview2"))
    );

    // Only this mock uses an absolute identifier: PathResolver's fallback is redirected
    // to a controlled sibling in the disposable fixture, never real OS AppData.
    // This bypasses application entry points and exercises the actual Tauri manager.
    context.config_mut().identifier = forbidden.to_string_lossy().into_owned();
    let mut app = tauri::test::mock_builder()
        .setup(move |app| {
            let mut wrong = deferred.clone();
            wrong[0].data_directory = Some(root.join("wrong-profile"));
            assert!(isolation::create_configured_windows(app, &wrong).is_err());
            assert!(!root.join("wrong-profile").exists());
            assert!(app.get_webview_window("ball").is_none());
            isolation::create_configured_windows(app, &deferred).map_err(std::io::Error::other)?;
            Ok(())
        })
        .build(context)
        .unwrap();
    app.run_iteration(|_, _| {});
    assert!(
        app.get_webview_window("ball").is_some(),
        "ball must survive bootstrap"
    );
    assert!(app.get_webview_window("not-auto-created").is_none());
    assert!(
        !forbidden.exists(),
        "real Tauri manager created a fallback outside the isolated root: {}",
        forbidden.display()
    );
}

#[cfg(not(feature = "beta2-isolation"))]
#[test]
fn ordinary_release_configuration_keeps_tauri_automatic_windows() {
    use deepseek_float_lib::isolation;
    let mut context = mock_context::<MockRuntime, _>(noop_assets());
    context.config_mut().identifier = "com.deepseekfloat.desktop".into();
    context.config_mut().app.windows.push(
        serde_json::from_value(serde_json::json!({
            "label":"ball", "url":"poc/ball.html", "visible":false
        }))
        .unwrap(),
    );
    let before = serde_json::to_value(context.config()).unwrap();
    assert!(isolation::configure_context(&mut context)
        .unwrap()
        .is_empty());
    assert_eq!(serde_json::to_value(context.config()).unwrap(), before);
    assert!(context.config().app.windows[0].create);
    assert!(isolation::initialize(
        [
            "test".into(),
            "--beta2-isolated-data-root".into(),
            "relative".into()
        ],
        "com.deepseekfloat.desktop"
    )
    .is_err());
}
