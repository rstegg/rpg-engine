-- RPG Engine: Initial schema for persistent character storage.

CREATE TABLE IF NOT EXISTS accounts (
    id          SERIAL PRIMARY KEY,
    username    VARCHAR(64) UNIQUE NOT NULL,
    created_at  TIMESTAMP DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS characters (
    id              SERIAL PRIMARY KEY,
    account_id      INTEGER NOT NULL REFERENCES accounts(id),
    name            VARCHAR(64) NOT NULL,
    appearance      JSONB NOT NULL,
    x               REAL NOT NULL DEFAULT 20.0,
    z               REAL NOT NULL DEFAULT 20.0,
    current_hp      INTEGER NOT NULL DEFAULT 100,
    max_hp          INTEGER NOT NULL DEFAULT 100,
    current_mp      INTEGER NOT NULL DEFAULT 100,
    max_mp          INTEGER NOT NULL DEFAULT 100,
    is_dead         BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMP DEFAULT NOW(),
    last_login      TIMESTAMP DEFAULT NOW(),
    UNIQUE(account_id, name)
);
