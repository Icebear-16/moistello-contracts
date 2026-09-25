#[cfg(test)]
mod tests {
    use super::super::gas_limits::{
        validate_batch_size, MAX_BATCH_INVITE_MEMBERS,
        MAX_RESOLVE_VOTE_ENTRIES,
    };

    #[test]
    fn batch_invite_maximum_is_accepted() {
        assert_eq!(
            validate_batch_size(MAX_BATCH_INVITE_MEMBERS, MAX_BATCH_INVITE_MEMBERS),
            Ok(())
        );
    }

    #[test]
    fn batch_invite_above_maximum_is_rejected() {
        assert!(validate_batch_size(
            MAX_BATCH_INVITE_MEMBERS + 1,
            MAX_BATCH_INVITE_MEMBERS
        ).is_err());
    }

    #[test]
    fn resolve_vote_maximum_is_accepted() {
        assert_eq!(
            validate_batch_size(MAX_RESOLVE_VOTE_ENTRIES, MAX_RESOLVE_VOTE_ENTRIES),
            Ok(())
        );
    }

    #[test]
    fn empty_batch_is_rejected() {
        assert!(validate_batch_size(0, MAX_BATCH_INVITE_MEMBERS).is_err());
    }
}
