CREATE TABLE IF NOT EXISTS user_follows (
    follower_user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    followee_user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (follower_user_id, followee_user_id),
    CONSTRAINT user_follows_no_self_follow CHECK (follower_user_id <> followee_user_id)
);

-- Who this account follows, newest first.
CREATE INDEX IF NOT EXISTS idx_user_follows_follower_created
    ON user_follows(follower_user_id, created_at DESC, followee_user_id DESC);

-- Who follows this account, newest first. Also serves the follower count and
-- the reverse lookup that decides whether a pair is mutual.
CREATE INDEX IF NOT EXISTS idx_user_follows_followee_created
    ON user_follows(followee_user_id, created_at DESC, follower_user_id DESC);
