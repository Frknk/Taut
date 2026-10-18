-- Signing secret of generic webhooks: the notifier sends `X-Taut-Signature` with an
-- HMAC-SHA256 of the body under this key, so the receiver can tell the request came from taut.
-- It is stored in clear because computing the HMAC needs the key itself (a hash would not do),
-- like the webhook URLs already kept in `config`. Its own column, not a `config` key: updating
-- a channel replaces `config` whole. Webhooks created before this migration stay unsigned until
-- their secret is rotated.

ALTER TABLE alert_channels
    ADD COLUMN signing_secret text
        CONSTRAINT alert_channels_signing_secret_check
        CHECK (signing_secret IS NULL OR kind = 'webhook');
