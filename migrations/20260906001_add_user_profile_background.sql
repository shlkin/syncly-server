-- Profile page background: the picture a member sets behind their own header.
--
-- Lives beside the signature in `user_profile_details` for the same reason that
-- table exists: only the two profile surfaces read it, and the projection that
-- backs authentication and every member list should not grow a column for it.
--
-- ON DELETE SET NULL rather than CASCADE: losing the stored object should leave
-- the profile on its default backdrop, not delete the row that holds the
-- signature next to it.
ALTER TABLE user_profile_details
    ADD COLUMN IF NOT EXISTS background_file_reference_id BIGINT NULL
        REFERENCES file_references(id) ON DELETE SET NULL;
