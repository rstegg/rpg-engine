use std::process::{Command, Child};
use std::time::Duration;
use std::thread;

use rpg_engine::net::protocol::*;
use rpg_engine::net::client::NetClient;

#[test]
fn test_headless_client_server() {
    // 1. Spawn server process on port 7879
    let server_process = Command::new("cargo")
        .args(["run", "--bin", "server", "--", "7879"])
        .spawn()
        .expect("Failed to start server");

    // Wait a bit for server to compile/start and bind to port
    // If it needs compiling it might take longer, but `cargo test` already compiles dependencies.
    thread::sleep(Duration::from_secs(5));

    // Cleanup server process when test ends (even on panic)
    struct ServerGuard(Child);
    impl Drop for ServerGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
        }
    }
    let mut guard = ServerGuard(server_process);

    // 2. Connect client
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
    
    let mut client = NetClient::connect("127.0.0.1:7879", "TestPlayer", appearance)
        .expect("Failed to connect to server");

    // 3. Wait for MapData
    let mut connected = false;
    for _ in 0..50 {
        client.update();
        if client.connected && client.pending_map.is_some() {
            connected = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(connected, "Client failed to join and receive MapData");
    
    // 4. Toggle God Mode
    client.send(&ClientMessage::DebugToggleGodMode);
    
    // 5. Test Movement
    client.send(&ClientMessage::MoveTo { x: 5.0, z: 5.0 });
    
    // Wait for WorldState to reflect movement
    let mut moved = false;
    for _ in 0..50 {
        client.update();
        if let Some(ref world) = client.latest_world {
            if let Some(my_id) = client.my_id {
                if let Some(player) = world.players.iter().find(|p| p.id == my_id) {
                    if (player.target_x - 5.0).abs() < 0.1 && (player.target_z - 5.0).abs() < 0.1 {
                        moved = true;
                        break;
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(moved, "Player failed to move");

    // 6. Force spawn enemy
    client.send(&ClientMessage::DebugForceSpawn);
    
    let mut enemy_id = None;
    for _ in 0..50 {
        client.update();
        if let Some(ref world) = client.latest_world {
            if !world.enemies.is_empty() {
                let enemy = &world.enemies[0];
                assert!(enemy.health > 0, "Enemy spawned dead");
                enemy_id = Some(enemy.id);
                break;
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(enemy_id.is_some(), "Enemy failed to spawn");

    // 7. Cast Kill All
    client.send(&ClientMessage::CastSpell { spell: 99, target_x: 0.0, target_z: 0.0 });
    
    let mut enemy_dead = false;
    for _ in 0..50 {
        client.update();
        if let Some(ref world) = client.latest_world {
            if let Some(enemy) = world.enemies.iter().find(|e| e.id == enemy_id.unwrap()) {
                if enemy.health <= 0 {
                    enemy_dead = true;
                    break;
                }
            } else {
                // If it's not in the array, it might be removed entirely
                enemy_dead = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    
    assert!(enemy_dead, "Kill All dev spell failed to kill enemy");
    let _ = guard.0.kill(); // Kill explicitly before ending

    println!("Integration test successful!");
}
