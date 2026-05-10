use crate::net::protocol::*;
use std::net::UdpSocket;
use std::time::Instant;

/// Client-side networking wrapper.
pub struct NetClient {
    socket: UdpSocket,
    pub my_id: Option<PlayerId>,
    pub connected: bool,
    pub latest_world: Option<WorldSnapshot>,
    pub remote_appearances: std::collections::HashMap<PlayerId, CharacterAppearanceNet>,
    pub ping_ms: f64,
    last_ping_time: Instant,
    pending_ping: Option<f64>,
    pub pending_map: Option<(Vec<(String, String)>, Vec<ModelPlacementNet>)>,
    pub latest_msg: Option<ServerMessage>,
}

/// A decoded world state from the server.
pub struct WorldSnapshot {
    pub tick: u64,
    pub players: Vec<PlayerState>,
    pub enemies: Vec<EnemyStateNet>,
    pub effects: Vec<EffectState>,
}

impl NetClient {
    /// Connect to a server. Non-blocking UDP socket.
    pub fn connect(
        server_addr: &str,
        name: &str,
        appearance: CharacterAppearanceNet,
    ) -> Result<Self, String> {
        let socket =
            UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("Failed to bind socket: {}", e))?;
        socket
            .connect(server_addr)
            .map_err(|e| format!("Failed to connect: {}", e))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking: {}", e))?;

        let client = Self {
            socket,
            my_id: None,
            connected: false,
            latest_world: None,
            remote_appearances: std::collections::HashMap::new(),
            ping_ms: 0.0,
            last_ping_time: Instant::now(),
            pending_ping: None,
            pending_map: None,
            latest_msg: None,
        };

        // Send join request
        let join_msg = ClientMessage::Join {
            version: PROTOCOL_VERSION,
            name: name.to_string(),
            appearance,
        };
        client.send(&join_msg);

        Ok(client)
    }

    /// Send a message to the server.
    pub fn send(&self, msg: &ClientMessage) {
        let packet = encode_client_message(msg);
        let _ = self.socket.send(&packet);
    }

    /// Poll for incoming server messages. Call this every frame.
    pub fn update(&mut self) {
        self.latest_msg = None;
        let mut buf = [0u8; 65536];

        // Read all pending packets
        loop {
            match self.socket.recv(&mut buf) {
                Ok(len) => {
                    if let Some(PacketPayload::Server(msg)) = decode_packet(&buf[..len]) {
                        self.handle_server_message(msg);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        // Send periodic pings (every 2 seconds)
        if self.connected && self.last_ping_time.elapsed().as_secs_f64() > 2.0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            self.pending_ping = Some(now);
            self.send(&ClientMessage::Ping { client_time: now });
            self.last_ping_time = Instant::now();
        }
    }

    fn handle_server_message(&mut self, msg: ServerMessage) {
        self.latest_msg = Some(msg.clone());
        match msg {
            ServerMessage::JoinAccepted { your_id } => {
                self.my_id = Some(your_id);
                self.connected = true;
                println!("[NET] Connected! Player ID: {}", your_id);
            }
            ServerMessage::JoinRejected { reason } => {
                eprintln!("[NET] Join rejected: {}", reason);
                self.connected = false;
            }
            ServerMessage::MapData { palette, placements } => {
                println!("[NET] Received MapData with {} placements ({} in palette)", 
                    placements.len(), palette.len());
                self.pending_map = Some((palette, placements));
            }
            ServerMessage::PlayerJoined {
                id,
                name,
                appearance,
            } => {
                println!("[NET] Player joined: {} ({})", name, id);
                self.remote_appearances.insert(id, appearance);
            }
            ServerMessage::PlayerLeft { id } => {
                println!("[NET] Player left: {}", id);
                self.remote_appearances.remove(&id);
            }
            ServerMessage::WorldState {
                tick,
                server_time: _,
                players,
                enemies,
                effects,
            } => {
                self.latest_world = Some(WorldSnapshot {
                    tick,
                    players,
                    enemies,
                    effects,
                });
            }
            ServerMessage::Pong {
                client_time,
                server_time: _,
            } => {
                if let Some(sent_time) = self.pending_ping {
                    if (sent_time - client_time).abs() < 0.001 {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs_f64();
                        self.ping_ms = (now - sent_time) * 1000.0;
                        self.pending_ping = None;
                    }
                }
            }
            ServerMessage::GameOver => {
                // GameState is updated in main.rs loop by checking latest_msg
            }
        }
    }

    /// Send a disconnect message before dropping.
    pub fn disconnect(&self) {
        self.send(&ClientMessage::Disconnect);
    }
}
