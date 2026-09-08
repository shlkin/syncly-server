-- Who may find this account, and who may read its follow graph.
--
-- Beside the signature in `user_profile_details` because these are read by the
-- same two surfaces (the public profile and the people search) and by nothing
-- on the authentication path. A user with no row yet is fully visible, which is
-- what every existing account was before this migration, so the defaults are
-- TRUE and the absence of a row is read the same way.
ALTER TABLE user_profile_details
    ADD COLUMN IF NOT EXISTS discoverable BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS show_following BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS show_followers BOOLEAN NOT NULL DEFAULT TRUE;

-- The search is `username ILIKE '%q%'`, which cannot use a b-tree index. Only
-- discoverable accounts are ever returned, so the partial index keeps the scan
-- to the rows that can match at all.
CREATE INDEX IF NOT EXISTS idx_user_profile_details_not_discoverable
    ON user_profile_details (user_id)
    WHERE discoverable = FALSE;
