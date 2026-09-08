-- Gifts now transfer points instead of burning them: sending one debits the
-- sender and credits the recipient the same amount, so a gift is worth what it
-- cost. This reverses the note in 20260905001, which burned the points to stop
-- two accounts from cycling the same balance between them at no cost. That
-- trade is now possible by design — a gift is a transfer, and the ledger shows
-- both halves so an admin can see it happen.
--
-- `total_earned` is deliberately NOT credited. It means "earned from the
-- system" and backs the lifetime figure on the profile; a gift moves points
-- that were already earned once, and counting them again on the way in would
-- let a pair inflate that number without limit.
-- The credit half of the send, mirroring `transaction_id` for the debit half.
-- 20260905001 already indexes `(recipient_id, created_at DESC, id DESC)`, which
-- is what both the profile showcase and the admin ledger read from.
ALTER TABLE gift_records
    ADD COLUMN IF NOT EXISTS recipient_transaction_id BIGINT NULL
        REFERENCES user_point_transactions(id) ON DELETE SET NULL;
