use crate::net::protocol::*;
use std::net::UdpSocket;
use std::time::Instant;

/// How long to wait for the server to respond during initial connection.
const CONNECTION_TIMEOUT_SECS: f64 = 5.0;

/// How long without a server packet before we consider the connection lost.
const DISCONNECT_TIMEOUT_SECS: f64 = 5.0;

/// Client-side networking wrapper.
pub struct NetClient {
    socket: UdpSocket,
    server_addr: String,
    pub my_id: Option<PlayerId>,
    pub connected: bool,
    pub latest_world: Option<WorldSnapshot>,
    pub remote_appearances: std::collections::HashMap<PlayerId, CharacterAppearanceNet>,
    pub ping_ms: f64,
    last_ping_time: Instant,
    pending_ping: Option<f64>,
    pub pending_map: Option<(Vec<(String, String)>, Vec<ModelPlacementNet>)>,
    pub latest_msg: Option<ServerMessage>,

    // ─── Connection robustness ───
    connect_start: Instant,
    last_server_packet: Instant,
    pub connection_timed_out: bool,
    pub server_lost: bool,
    pub session_token: Option<u64>,

    // ─── Character select flow ───
    pub pending_characters: Option<Vec<CharacterSummaryNet>>,
    pub pending_character_created: Option<CharacterSummaryNet>,
    pub pending_create_error: Option<String>,
    pub pending_character_deleted: Option<i32>,

    // ─── Reconnection ───
    pub reconnect_attempts: u32,
    pub max_reconnect_attempts: u32,
}

/// A decoded world state from the server.
pub struct WorldSnapshot {
    pub tick: u64,
    pub players: Vec<PlayerState>,
    pub enemies: Vec<EnemyStateNet>,
    pub effects: Vec<EffectState>,
}

impl NetClient {
    /// Connect to a server and send a Login message. Non-blocking UDP socket.
    pub fn connect(server_addr: &str, username: &str) -> Result<Self, String> {
        let socket =
            UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("Failed to bind socket: {}", e))?;
        socket
            .connect(server_addr)
            .map_err(|e| format!("Failed to connect: {}", e))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking: {}", e))?;

        let now = Instant::now();
        let client = Self {
            socket,
            server_addr: server_addr.to_string(),
            my_id: None,
            connected: false,
            latest_world: None,
            remote_appearances: std::collections::HashMap::new(),
            ping_ms: 0.0,
            last_ping_time: now,
            pending_ping: None,
            pending_map: None,
            latest_msg: None,
            connect_start: now,
            last_server_packet: now,
            connection_timed_out: false,
            server_lost: false,
            session_token: None,
            pending_characters: None,
            pending_character_created: None,
            pending_create_error: None,
            pending_character_deleted: None,
            reconnect_attempts: 0,
            max_reconnect_attempts: 5,
        };

        // Send login request
        let login_msg = ClientMessage::Login {
            version: PROTOCOL_VERSION,
            username: username.to_string(),
        };
        client.send(&login_msg);

        Ok(client)
    }

    /// Attempt to reconnect using the stored session token.
    /// Re-binds a new socket and sends a Reconnect message.
    pub fn reconnect(&mut self) -> Result<(), String> {
        let token = self.session_token.ok_or("No session token for reconnect")?;

        let socket =
            UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("Failed to bind socket: {}", e))?;
        socket
            .connect(&self.server_addr)
            .map_err(|e| format!("Failed to connect: {}", e))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to set nonblocking: {}", e))?;

        self.socket = socket;
        self.connected = false;
        self.server_lost = false;
        self.connection_timed_out = false;
        let now = Instant::now();
        self.connect_start = now;
        self.last_server_packet = now;
        self.reconnect_attempts += 1;

        let msg = ClientMessage::Reconnect {
            session_token: token,
        };
        self.send(&msg);

        Ok(())
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
                        self.last_server_packet = Instant::now();
                        self.handle_server_message(msg);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        // Connection timeout check (waiting for initial response)
        if !self.connected
            && !self.connection_timed_out
            && self.connect_start.elapsed().as_secs_f64() > CONNECTION_TIMEOUT_SECS
        {
            self.connection_timed_out = true;
        }

        // Disconnect detection (server went silent)
        if self.connected
            && !self.server_lost
            && self.last_server_packet.elapsed().as_secs_f64() > DISCONNECT_TIMEOUT_SECS
        {
            self.server_lost = true;
            self.connected = false;
            println!("[NET] Lost connection to server (no packets for {:.0}s)", DISCONNECT_TIMEOUT_SECS);
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
            ServerMessage::JoinAccepted {
                your_id,
                session_token,
            } => {
                self.my_id = Some(your_id);
                self.connected = true;
                self.session_token = Some(session_token);
                self.reconnect_attempts = 0;
                println!(
                    "[NET] Connected! Player ID: {}, Session: {:016x}",
                    your_id, session_token
                );
            }
            ServerMessage::JoinRejected { reason } => {
                eprintln!("[NET] Join rejected: {}", reason);
                self.connected = false;
            }
            ServerMessage::CharacterList { characters } => {
                println!(
                    "[NET] Received character list ({} characters)",
                    characters.len()
                );
                self.pending_characters = Some(characters);
            }
            ServerMessage::CharacterCreated { character } => {
                println!("[NET] Character created: {} (id={})", character.name, character.id);
                self.pending_character_created = Some(character);
            }
            ServerMessage::CharacterCreateFailed { reason } => {
                eprintln!("[NET] Character creation failed: {}", reason);
                self.pending_create_error = Some(reason);
            }
            ServerMessage::CharacterDeleted { character_id } => {
                println!("[NET] Character deleted: id={}", character_id);
                self.pending_character_deleted = Some(character_id);
            }
            ServerMessage::MapData {
                palette,
                placements,
            } => {
                println!(
                    "[NET] Received MapData with {} placements ({} in palette)",
                    placements.len(),
                    palette.len()
                );
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

    /// How many seconds have elapsed since the connection attempt started.
    pub fn connecting_elapsed(&self) -> f64 {
        self.connect_start.elapsed().as_secs_f64()
    }

    /// Send a disconnect message before dropping.
    pub fn disconnect(&self) {
        self.send(&ClientMessage::Disconnect);
    }
}
