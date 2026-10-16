-- Second factor (identity PR 2): a TOTP secret per user, encrypted with AES-256-GCM under
-- TAUT_AUTH__TOTP_KEY (NULL until the user confirms the enrollment), and the backup codes that
-- replace it when the phone is gone. Identity tables: no tenant_id (docs/adr/0008).

ALTER TABLE users
    ADD COLUMN totp_secret     text,
    ADD COLUMN totp_enabled_at timestamptz;

-- Only the SHA-256 of each code is stored; a used code keeps its row with used_at set, so the
-- panel can say how many are left.
CREATE TABLE mfa_backup_codes (
    user_id    uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code_hash  text NOT NULL,
    used_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, code_hash)
);
