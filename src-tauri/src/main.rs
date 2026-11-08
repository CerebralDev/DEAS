#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod linux_startup;

#[cfg(not(test))]
fn main() {
    #[cfg(target_os = "linux")]
    linux_startup::configure();
    #[cfg(target_os = "linux")]
    linux_startup::provide_gstreamer_plugins();

    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;
            // The shared Engine Arc is the only authority admissions may consult for owned
            // backends and environments.
            deas::bind_resource_authority(app.state::<deas::install::Engine>().inner().clone());
            match app.path().app_data_dir() {
                Ok(path) => app.state::<deas::jobs::Jobs>().initialize_history(&path),
                Err(_) => app
                    .state::<deas::jobs::Jobs>()
                    .initialize_history_unavailable(),
            }
            deas::monitoring::spawn_job_sampler(app.state::<deas::jobs::Jobs>().inner().clone());
            let config = app.config().app.windows[0].clone();
            let mut bridge = String::new();
            #[cfg(debug_assertions)]
            if let Some(url) = &app.config().build.dev_url {
                bridge.push_str(&format!(
                    "const DEAS_DEV_ORIGIN = {};\n",
                    serde_json::to_string(&url.origin().ascii_serialization())?
                ));
            }
            bridge.push_str(include_str!("comfy_bridge.js"));
            // Local verification only; release binaries never execute an injected probe.
            #[cfg(debug_assertions)]
            if let Some(path) = std::env::var_os("DEAS_VIDEO_TEST_SCRIPT") {
                use tauri::{Listener, Manager};
                app.add_capability(
                    tauri::ipc::CapabilityBuilder::new("video-probe")
                        .window("main")
                        .permission("core:event:allow-emit"),
                )?;
                app.listen("deas-video-probe", |event| {
                    eprintln!("DEAS_VIDEO_PROBE {}", event.payload())
                });
                bridge.push_str(&std::fs::read_to_string(path)?);
            }
            deas::splash::open(app);
            // Main stays hidden until the UI reports its state restored (or the splash deadline passes).
            tauri::WebviewWindowBuilder::from_config(app, &config)?
                .visible(false)
                .on_page_load(|webview, payload| {
                    if payload.event() == tauri::webview::PageLoadEvent::Finished {
                        deas::splash::phase(webview.app_handle(), "interface");
                    }
                })
                .initialization_script_for_all_frames(bridge)
                .build()?;
            Ok(())
        })
        .manage(deas::install::Engine::default())
        .manage(deas::FolderOperations::default())
        .manage(deas::jobs::Jobs::default())
        .manage(deas::projects::Projects::default())
        .manage(deas::auth::AuthState::default().restore_saved())
        .manage(deas::downloads::DownloadEngine::default())
        .manage(deas::ModelScanEngine::default())
        .invoke_handler(tauri::generate_handler![
            deas::spellcheck::native_spellcheck_status,
            deas::spellcheck::native_spellcheck_install,
            deas::spellcheck::native_spellcheck_check,
            deas::spellcheck::native_spellcheck_suggest,
            deas::native_audio_enhance_capabilities,
            deas::native_audio_enhance_export,
            deas::native_audio_enhance_cancel,
            deas::native_audio_enhance_install,
            deas::native_audio_enhance_install_cancel,
            deas::native_audio_enhance_install_status,
            deas::native_video_post_inspect,
            deas::native_video_post_inspect_path,
            deas::native_video_post_upload,
            deas::video::native_video_open,
            deas::video::native_video_update,
            deas::native_auth_status,
            deas::native_auth_verify,
            deas::native_auth_set,
            deas::native_auth_remove,
            deas::native_auth_restore,
            deas::native_download_catalog,
            deas::native_download_import_catalog,
            deas::native_download_browse,
            deas::native_download_plan,
            deas::native_download_start,
            deas::native_download_status,
            deas::native_download_cancel,
            deas::native_models_scan,
            deas::native_model_info,
            deas::native_model_family_write,
            deas::native_model_scan_start,
            deas::native_model_scan_status,
            deas::inspect_backend,
            deas::native_negotiate_api,
            deas::native_recipe_prerequisites,
            deas::native_workbench_cancel_pending,
            deas::native_workbench_interrupt_running,
            deas::native_workbench_current,
            deas::native_workbench_poll,
            deas::native_workbench_run,
            deas::native_auto_refine,
            deas::native_theme_packs,
            deas::native_theme_editor,
            deas::native_theme_folders,
            deas::native_theme_read,
            deas::native_theme_write,
            deas::native_workbench_stop,
            deas::native_jobs_history,
            deas::native_jobs_history_outputs,
            deas::native_logs_history_error,
            deas::native_job_current,
            deas::native_job_output,
            deas::native_job_export,
            deas::native_job_prepare,
            deas::native_job_submit,
            deas::native_mask_upload,
            deas::native_input_upload,
            deas::native_h3_session,
            deas::native_h3_video_prepare,
            deas::native_h3_video_upload,
            deas::native_h3_refmods,
            deas::native_h3_refmod_submit,
            deas::native_h3_refmod_status,
            deas::native_h3_refmod_cancel,
            deas::native_job_status,
            deas::native_project_create,
            deas::native_project_open,
            deas::native_project_save,
            deas::native_project_import_image,
            deas::native_project_export_package,
            deas::native_project_import_package,
            deas::native_project_close,
            deas::native_project_close_notice,
            deas::native_pick_directory,
            deas::native_json_save,
            deas::native_json_open,
            deas::native_media_list,
            deas::native_media_star,
            deas::native_media_unstar,
            deas::native_media_star_status,
            deas::native_media_star_notice,
            deas::native_media_describe,
            deas::native_media_thumbnail,
            deas::native_media_paste,
            deas::native_media_copy_in,
            deas::native_media_reference_thumbnail,
            deas::native_media_reveal,
            deas::native_media_metadata,
            deas::native_open_token_page,
            deas::native_open_repo,
            deas::native_repo_stars,
            deas::native_repo_star,
            deas::native_open_releases,
            deas::native_update_check,
            deas::native_app_update,
            deas::native_app_restart,
            deas::native_settings_read,
            deas::native_presets_read,
            deas::native_presets_write,
            deas::native_presets_invalid,
            deas::native_settings_write,
            deas::native_settings_move,
            deas::native_environment_move,
            deas::native_startup_read,
            deas::native_startup_write,
            deas::native_llm_read,
            deas::native_llm_write,
            deas::native_llm_generate,
            deas::native_llm_models,
            deas::native_forge,
            deas::forge::engine::native_forge_write,
            deas::forge::engine::native_forge_cancel,
            deas::forge::engine::native_forge_revise,
            deas::forge::engine::native_forge_layout_rewrite,
            deas::forge::assistant::native_chat_send,
            deas::forge::assistant::native_chat_cancel,
            deas::forge::assistant::native_chat_list,
            deas::forge::assistant::native_chat_get,
            deas::forge::assistant::native_chat_rename,
            deas::forge::assistant::native_chat_delete,
            deas::forge::assistant::native_chat_settings,
            deas::forge::assistant::native_chat_settings_write,
            deas::forge::assistant::native_chat_render_result,
            deas::forge::assistant::native_chat_free_llm,
            deas::forge::assistant::native_chat_pin,
            deas::forge::engine::native_forge_reference_readback,
            deas::forge::randomizer::native_forge_randomizer_list,
            deas::forge::randomizer::native_forge_randomizer_roll,
            deas::forge::picker::native_forge_tag_search,
            deas::forge::picker::native_forge_character_traits,
            deas::forge::engine::native_forge_targets,
            deas::native_wildcard_library,
            deas::native_wildcard_build,
            deas::native_files_present,
            deas::native_detect_hardware,
            deas::native_monitoring_sample,
            deas::native_install_defaults,
            deas::native_install_plan,
            deas::native_install_start,
            deas::native_install_open,
            deas::native_install_status,
            deas::native_install_cancel,
            deas::native_install_update,
            deas::native_torch_update_check,
            deas::native_logs_beginner_error,
            deas::native_logs,
            deas::native_logs_export,
            deas::native_logs_people_image_error,
            deas::native_logs_reference_media_error,
            deas::native_backend_start,
            deas::native_backend_status,
            deas::native_backend_stop,
            deas::native_backend_test,
            deas::splash::native_startup_phase,
            deas::splash::native_startup_ready
        ])
        .build(tauri::generate_context!())
        .expect("failed to build DEAS")
        .run(|app, event| {
            use tauri::Manager;
            if matches!(event, tauri::RunEvent::Ready) {
                deas::video::install(app);
            }
            if matches!(event, tauri::RunEvent::Exit) {
                deas::video::shutdown();
                app.state::<deas::jobs::Jobs>().flush_history();
            }
            if matches!(
                event,
                tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }
            ) {
                app.state::<deas::jobs::Jobs>().cancel_audio_stages();
                deas::native_audio_enhance_install_cancel();
                deas::audio_enhance::shutdown();
                app.state::<deas::install::Engine>().shutdown_backend();
                let _ = app.state::<deas::downloads::DownloadEngine>().cancel();
            }
        });
}

#[cfg(test)]
fn main() {}
