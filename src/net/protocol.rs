use serde::{Deserialize, Serialize};

/// Maximum players the server will accept.
pub const MAX_PLAYERS: usize = 16;

/// Server tick rate in Hz (how many times per second the server broadcasts world state).
pub const SERVER_TICK_RATE: u64 = 20;

/// Default server port.
pub const DEFAULT_PORT: u16 = 7878;

/// Magic bytes prepended to every packet for basic validation.
pub const PROTOCOL_MAGIC: [u8; 4] = [0x52, 0x50, 0x47, 0x45]; // "RPGE"

/// Protocol version — clients and server must match.
pub const PROTOCOL_VERSION: u8 = 3;

/// Maximum characters per account.
pub const MAX_CHARACTERS_PER_ACCOUNT: usize = 5;

// ─── Unique Player Identity ───

pub type PlayerId = u64;

// ─── Client → Server Messages ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Login with a username. Auto-creates account if new.
    Login {
        version: u8,
        username: String,
    },
    /// Select an existing character to play.
    SelectCharacter {
        character_id: i32,
    },
    /// Create a new character on the account.
    CreateCharacter {
        name: String,
        appearance: CharacterAppearanceNet,
    },
    /// Delete a character from the account.
    DeleteCharacter {
        character_id: i32,
    },
    /// Reconnect using a session token after disconnect.
    Reconnect {
        session_token: u64,
    },
    /// Player wants to move to this world position.
    MoveTo { x: f32, z: f32 },
    /// Player wants to cast a spell.
    CastSpell {
        spell: u8, // 0=Q, 1=W, 2=E, 3=R
        target_x: f32,
        target_z: f32,
    },
    /// Heartbeat to keep connection alive.
    Ping { client_time: f64 },
    /// Player is leaving gracefully.
    Disconnect,
    /// DEBUG: Toggle invulnerability.
    DebugToggleGodMode,
    /// DEBUG: Force spawn an enemy nearby for testing.
    DebugForceSpawn,
}

// ─── Server → Client Messages ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Welcome! Here is your assigned player ID and session token.
    JoinAccepted {
        your_id: PlayerId,
        session_token: u64,
    },
    /// The map data (placements).
    MapData {
        palette: Vec<(String, String)>, // (model, file)
        placements: Vec<ModelPlacementNet>,
    },
    /// A specific chunk of map data.
    ChunkData {
        coord_x: i32,
        coord_z: i32,
        biome: String,
        palette: Vec<(String, String)>,
        placements: Vec<ModelPlacementNet>,
    },
    /// Server is full or version mismatch.
    JoinRejected { reason: String },
    /// Your character list after login.
    CharacterList {
        characters: Vec<CharacterSummaryNet>,
    },
    /// A character was successfully created.
    CharacterCreated {
        character: CharacterSummaryNet,
    },
    /// Character creation failed.
    CharacterCreateFailed { reason: String },
    /// Character was deleted.
    CharacterDeleted { character_id: i32 },
    /// A new player has joined.
    PlayerJoined {
        id: PlayerId,
        name: String,
        appearance: CharacterAppearanceNet,
    },
    /// A player has left.
    PlayerLeft { id: PlayerId },
    /// The authoritative world state snapshot. Sent every tick.
    WorldState {
        tick: u64,
        server_time: f64,
        players: Vec<PlayerState>,
        enemies: Vec<EnemyStateNet>,
        effects: Vec<EffectState>,
        gates: Vec<GateStateNet>,
    },
    /// Response to client Ping.
    Pong { client_time: f64, server_time: f64 },
    /// Everyone is dead.
    GameOver,
}

// ─── Shared Data Structures ───

/// Summary of a character for the select screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterSummaryNet {
    pub id: i32,
    pub name: String,
    pub appearance: CharacterAppearanceNet,
    pub current_hp: i32,
    pub max_hp: i32,
    pub current_mp: i32,
    pub max_mp: i32,
}

/// Compact player state for network transmission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: PlayerId,
    pub x: f32,
    pub z: f32,
    pub target_x: f32,
    pub target_z: f32,
    pub direction: u8,  // 0-7 matching Direction enum
    pub anim_state: u8, // Matches AnimationState enum
    pub anim_frame: f32,
    pub casting_timer: f32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub current_mp: i32,
    pub max_mp: i32,
    pub is_dead: bool,
    pub revive_progress: f32, // 0.0 to 1.0
}

/// Compact spell effect for network transmission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectState {
    pub effect_id: u64,
    pub spell: u8,
    pub x: f32,
    pub z: f32,
    pub timer: f32,
    pub caster_id: PlayerId,
}

/// Compact enemy state for network transmission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnemyStateNet {
    pub id: u64,
    pub race_idx: usize,
    pub x: f32,
    pub z: f32,
    pub target_x: f32,
    pub target_z: f32,
    pub direction: u8,
    pub anim_state: u8,
    pub health: i32,
    pub max_health: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateStateNet {
    pub x: f32,
    pub z: f32,
    pub open_progress: f32, // 0.0 closed, 1.0 open
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPlacementNet {
    pub model_idx: u16,
    pub position: [f32; 3],
    pub rotation: f32,
    pub scale: f32,
    pub blocks_movement: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChunkDataNet {
    pub coord_x: i32,
    pub coord_z: i32,
    pub biome: String,
    pub palette: Vec<(String, String)>,
    pub placements: Vec<ModelPlacementNet>,
}

/// Network-friendly character appearance (mirrors CharacterAppearance but lightweight).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterAppearanceNet {
    pub skin: String,
    pub shoes: Option<String>,
    pub clothes: Option<String>,
    pub gloves: Option<String>,
    pub hairstyle: Option<String>,
    pub facial_hair: Option<String>,
    pub eye_color: Option<String>,
    pub eyelashes: Option<String>,
    pub headgear: Option<String>,
    pub addon: Option<String>,
}

// ─── Packet Framing ───

/// Encode a message into a packet with magic header + version.
pub fn encode_client_message(msg: &ClientMessage) -> Vec<u8> {
    let mut packet = Vec::with_capacity(512);
    packet.extend_from_slice(&PROTOCOL_MAGIC);
    packet.push(PROTOCOL_VERSION);
    packet.push(0x01); // Message type: Client
    let payload = bincode::serialize(msg).expect("Failed to serialize client message");
    packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packet.extend_from_slice(&payload);
    packet
}

pub fn encode_server_message(msg: &ServerMessage) -> Vec<u8> {
    let mut packet = Vec::with_capacity(2048);
    packet.extend_from_slice(&PROTOCOL_MAGIC);
    packet.push(PROTOCOL_VERSION);
    packet.push(0x02); // Message type: Server
    let payload = bincode::serialize(msg).expect("Failed to serialize server message");
    packet.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    packet.extend_from_slice(&payload);
    packet
}

/// Decode a raw packet. Returns None if invalid.
pub fn decode_packet(data: &[u8]) -> Option<PacketPayload> {
    if data.len() < 10 {
        return None;
    } // Magic(4) + Version(1) + Type(1) + Len(4)
    if &data[0..4] != &PROTOCOL_MAGIC {
        return None;
    }
    if data[4] != PROTOCOL_VERSION {
        return None;
    }

    let msg_type = data[5];
    let payload_len = u32::from_le_bytes([data[6], data[7], data[8], data[9]]) as usize;

    if data.len() < 10 + payload_len {
        return None;
    }
    let payload = &data[10..10 + payload_len];

    match msg_type {
        0x01 => bincode::deserialize::<ClientMessage>(payload)
            .ok()
            .map(PacketPayload::Client),
        0x02 => bincode::deserialize::<ServerMessage>(payload)
            .ok()
            .map(PacketPayload::Server),
        _ => None,
    }
}

#[derive(Debug)]
pub enum PacketPayload {
    Client(ClientMessage),
    Server(ServerMessage),
}
