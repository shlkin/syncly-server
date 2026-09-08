-- Public profile text that only the profile surfaces read.
--
-- Kept out of the `users` table and the `user_account_profiles` view on purpose:
-- that projection backs authentication, room member lists, and admin listings,
-- so widening it would rewrite every cached query for a field two pages need.
CREATE TABLE IF NOT EXISTS user_profile_details (
    user_id BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    -- Empty means "no signature yet"; the surfaces show their own invitation copy.
    signature VARCHAR(200) NOT NULL DEFAULT '',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
