//! Fieldnotes Tauri application entry point.
//!
//! ## Module overview
//!
//! - `commands` — Tauri commands (frontend ↔ backend bridge). **Edit this for your app.**
//! - `conductor` — Holochain conductor lifecycle (start, stop, health). Reusable.
//! - `dna` — DNA installation and multi-version management. Change app IDs only.
//! - `lair` — Lair keystore management. Reusable as-is.
//! - `migration` — DNA version migration orchestration. Adapt entry types.
//! - `crypto` — Lair-based encryption for private data on public DHT.
//!
//! ## Startup sequence
//!
//! 1. Create data directory and AppState
//! 2. Spawn async: start lair → start conductor → install DNAs → setup WebSockets
//! 3. If migration needed: spawn async migration task
//! 4. If pending votes from previous run: spawn retry loop
//!
//! ## For forking developers
//!
//! Update the `invoke_handler` at the bottom to register your own Tauri commands.
//! The startup sequence and migration wiring are reusable as-is.


mod commands;
mod conductor;
mod crypto;
mod dna;
mod lair;
pub mod migration;
mod process_ext;
mod sidecar;

use commands::AppState;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");
            std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");

            // Log to file (and stdout in dev) at INFO level in both dev and
            // release builds. Release-build silence would make any startup
            // failure invisible to users (and to us); the cost of a small
            // rolling log file is worth the diagnosability.
            app.handle().plugin(
                tauri_plugin_log::Builder::default()
                    .level(log::LevelFilter::Info)
                    .targets([
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                            file_name: Some("fieldnotes".to_string()),
                        }),
                        tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    ])
                    .build(),
            )?;

            log::info!("Fieldnotes starting up...");
            log::info!("Data dir: {:?}", data_dir);

            let app_state = Arc::new(AppState::new(data_dir));
            app.manage(app_state.clone());

            // Resolve the resource directory where the .happ bundle lives.
            // In dev mode, Tauri doesn't copy resources to target/debug/,
            // so point directly at the source resources/ folder.
            // In release, Tauri's resource_dir() returns the app's root
            // install directory; entries declared as `resources/foo` in
            // tauri.conf.json land under a `resources/` subdir there, so
            // we append that to match dev's layout.
            #[cfg(debug_assertions)]
            let resource_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources");
            #[cfg(not(debug_assertions))]
            let resource_dir = app
                .path()
                .resource_dir()
                .expect("Failed to get resource directory")
                .join("resources");

            // Auto-start the conductor in the background.
            let startup_state = app_state.clone();
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                {
                    let mut status = startup_state.conductor_status.lock().unwrap();
                    *status = conductor::ConductorStatus::Starting {
                        message: "Initializing...".into(),
                    };
                }

                let passphrase = startup_state.passphrase.lock().unwrap().clone();
                let data_dir = startup_state.data_dir.clone();

                let monitor_handle = app_handle.clone();
                let migration_handle = app_handle.clone();
                match conductor::start_holochain(
                    app_handle,
                    startup_state.clone(),
                    data_dir,
                    resource_dir,
                    passphrase,
                )
                .await
                {
                    Ok(result) => {
                        log::info!("Conductor ready, awaiting network choice");

                        #[cfg(target_os = "windows")]
                        {
                            use tauri::Manager;
                            if let Some(win) = monitor_handle.get_webview_window("main") {
                                let _ = win.set_focus();
                            }
                        }

                        let admin_port = result.handle.admin_port;
                        let conductor_pid = result.handle.conductor_pid;

                        // PHASE 1: store conductor handle + lair client only. No
                        // DNA installed yet; the user chooses a network first.
                        // Agent key, app clients and app_port are set in phase 2
                        // (install_network) after that choice.
                        *startup_state.conductor_handle.lock().unwrap() = Some(result.handle);
                        *startup_state.lair_client.lock().await = Some(result.lair_client);
                        *startup_state.conductor_status.lock().unwrap() =
                            conductor::ConductorStatus::AwaitingNetwork { admin_port };

                        conductor::spawn_health_monitor(
                            conductor_pid,
                            startup_state.clone(),
                            monitor_handle,
                        );

                        let _ = &migration_handle;
                    }
                    Err(e) => {
                        log::error!("Failed to start conductor: {}", e);
                        let mut status = startup_state.conductor_status.lock().unwrap();
                        *status = conductor::ConductorStatus::Error { message: e };
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // ── Infrastructure (keep as-is) ───────────────────────
            commands::get_app_status,
            commands::launch_vault,
            commands::app_environment,
            commands::install_network,
            commands::get_network_info,
            // ── Fieldnotes: scenarios / responses / findings ──────
            commands::create_item,
            commands::import_items,
            commands::get_item,
            commands::get_all_items,
            commands::archive_item,
            commands::get_archived_items,
            commands::unarchive_item,
            commands::respond,
            commands::get_item_responses,
            commands::create_finding,
            commands::get_item_findings,
            // ── Encrypted attachments ─────────────────────────────
            commands::create_encrypted_attachment,
            commands::get_finding_attachments,
            commands::decrypt_attachment,
            // delete_poll: no Item delete in v0.0.1 — command kept dormant
            // in commands.rs but intentionally not registered here.
            // ── Administrator functions (new) ─────────────────────
            commands::add_administrator,
            commands::pubkey_raw_b64,
            commands::is_administrator,
            commands::get_administrators,
            commands::get_admin_grant_hash,
            // ── Community moderation (keep or adapt) ──────────────
            commands::flag_poll,
            commands::get_poll_flags,
            commands::remove_flag,
            commands::get_flag_threshold,
            // ── Flowsta identity linking (keep as-is) ─────────────
            commands::get_cached_profile,
            commands::save_profile_cache,
            commands::commit_identity_link,
            commands::get_linked_agents,
            commands::get_my_agent_set,
            commands::get_identity_link,
            commands::revoke_identity_link,
            // ── Data export + migration (keep as-is) ──────────────
            commands::get_migration_status,
            commands::abandon_pending_votes,
            // ── CAL-compliant backup + reinstall recovery ──────────
            // See build-docs/current/LAIR_RECOVERY_AND_CAL_COMPLIANCE.md
            commands::decode_record_for_export,
            commands::build_canonical_backup,
            // ── Encrypted entries (v1.3) ───────────────────────────
            commands::save_vote_rationale,
            commands::get_vote_rationale,
            commands::save_draft_poll,
            commands::get_my_drafts,
            commands::publish_draft,
            commands::delete_draft,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
