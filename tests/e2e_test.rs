use std::process::{Command, Child};
use std::time::Duration;
use std::thread;

use rpg_engine::net::protocol::*;
use rpg_engine::net::client::NetClient;

#[test]
fn test_headless_client_server() {
    let target_dir = std::env::var("CARGO_TARGET_DIR")
        .unwrap_or_else(|_| "target/e2e-test-target".to_string());

    // 1. Spawn server process on port 7879
    let server_process = Command::new("cargo")
        .args(["run", "--bin", "server", "--", "7879"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .spawn()
        .expect("Failed to start server");

    // Cleanup server process when test ends (even on panic)
    struct ServerGuard(Child);
    impl Drop for ServerGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
        }
    }
    let mut guard = ServerGuard(server_process);

    let appearance = CharacterAppearanceNet {
        skin: "human".to_string(),
        shoes: None,
        clothes: None,
        gloves: None,
        hairstyle: None,
        facial_hair: None,
        eye_color: None,
        eyelashes: None,
        headgear: None,
        addon: None,
    };
    // 2. Connect client once the server is actually ready.
    let mut client = None;
    for _ in 0..240 {
        match NetClient::connect("127.0.0.1:7879", "TestPlayer") {
            Ok(connected) => {
                client = Some(connected);
                break;
            }
            Err(_) => thread::sleep(Duration::from_millis(500)),
        }
    }
    let mut client = client.expect("Failed to connect to server");

    // 3. Select character flow (Wait for list, select first or create)
    let mut joined = false;
    for _ in 0..100 {
        client.update();
        if client.connection_timed_out && !client.connected {
            client = NetClient::connect("127.0.0.1:7879", "TestPlayer")
                .expect("Failed to reconnect to server during join flow");
        }
        if let Some(chars) = client.pending_characters.take() {
            if chars.is_empty() {
                client.send(&ClientMessage::CreateCharacter {
                    name: "TestHero".into(),
                    appearance: appearance.clone(),
                });
            } else {
                client.send(&ClientMessage::SelectCharacter { character_id: chars[0].id });
            }
        }
        if let Some(created) = client.pending_character_created.take() {
            client.send(&ClientMessage::SelectCharacter { character_id: created.id });
        }
        if client.connected {
            joined = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(joined, "Client failed to join the game world");

    let my_id = client.my_id.expect("client should have an assigned player id");
    let mut start_pos = None;
    for _ in 0..50 {
        client.update();
        if let Some(ref world) = client.latest_world {
            if let Some(player) = world.players.iter().find(|p| p.id == my_id) {
                start_pos = Some((player.x, player.z));
                break;
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    let start_pos = start_pos.expect("Failed to observe the local player in world state");
    
    // 4. Toggle God Mode
    client.send(&ClientMessage::DebugToggleGodMode);
    
    // 5. Test Movement
    client.send(&ClientMessage::MoveTo { x: 5.0, z: 5.0 });
    
    // Wait for WorldState to reflect movement
    let mut moved = false;
    let start_dist_to_goal = ((start_pos.0 - 5.0).powi(2) + (start_pos.1 - 5.0).powi(2)).sqrt();
    for _ in 0..50 {
        client.update();
        if let Some(ref world) = client.latest_world {
            if let Some(player) = world.players.iter().find(|p| p.id == my_id) {
                let current_dist_to_goal = ((player.x - 5.0).powi(2) + (player.z - 5.0).powi(2)).sqrt();
                if current_dist_to_goal + 1.0 < start_dist_to_goal {
                    moved = true;
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(moved, "Player failed to move");

    let _ = guard.0.kill(); // Kill explicitly before ending

    println!("Integration movement test successful!");
}
