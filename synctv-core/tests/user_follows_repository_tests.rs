//! Follow graph integration tests.
//!
//! Covers the edge table's set semantics, the profile counters, and the two
//! listings, including how a soft-deleted account drops out of both.
//!
//! Run Docker tests: cargo test --test `user_follows_repository_tests` -- --ignored

use chrono::Utc;
use synctv_core::{
    models::{PageParams, SignupMethod, User, UserId, UserRole, UserStatus},
    repository::UserRepository,
};
use synctv_core_testing::{create_test_pool, ok};

fn make_user(username: &str) -> User {
    let now = Utc::now();
    User {
        id: UserId::new(),
        username: username.to_string(),
        role: UserRole::User,
        avatar_file_reference_id: None,
        status: UserStatus::Active,
        signup_method: SignupMethod::Email,
        created_at: now,
        updated_at: now,
        version: 0,
        deleted_at: None,
        is_banned: false,
        banned_at: None,
        banned_by: None,
        banned_reason: None,
    }
}

async fn create_user(repo: &UserRepository, username: &str) -> User {
    ok(
        repo.create(&make_user(username)).await,
        "follow test user should be created",
    )
}

fn first_page() -> PageParams {
    PageParams::new(Some(1), Some(50))
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_follow_is_idempotent_and_unfollow_is_a_no_op_for_strangers() {
    let (_container, pool) = create_test_pool().await;
    let repo = UserRepository::new(pool.clone());
    let follower = create_user(&repo, "follow_idem_follower").await;
    let followee = create_user(&repo, "follow_idem_followee").await;

    let first = ok(
        repo.follow_user(&follower.id, &followee.id).await,
        "first follow should be recorded",
    );
    let second = ok(
        repo.follow_user(&follower.id, &followee.id).await,
        "second follow should return the original edge",
    );
    assert_eq!(
        first, second,
        "following twice must not move the creation timestamp"
    );

    assert!(ok(
        repo.is_following(&follower.id, &followee.id).await,
        "follow edge should be readable"
    ));
    assert!(
        !ok(
            repo.is_following(&followee.id, &follower.id).await,
            "reverse edge should be readable"
        ),
        "following is one-directional until the other side follows back"
    );

    assert!(ok(
        repo.unfollow_user(&follower.id, &followee.id).await,
        "unfollow should report the removed edge"
    ));
    assert!(
        !ok(
            repo.unfollow_user(&follower.id, &followee.id).await,
            "second unfollow should succeed"
        ),
        "unfollowing a stranger is a no-op, not an error"
    );
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_follow_profile_stats_counts_edges_and_viewer_relationship() {
    let (_container, pool) = create_test_pool().await;
    let repo = UserRepository::new(pool.clone());
    let subject = create_user(&repo, "follow_stats_subject").await;
    let viewer = create_user(&repo, "follow_stats_viewer").await;
    let other = create_user(&repo, "follow_stats_other").await;

    ok(
        repo.follow_user(&viewer.id, &subject.id).await,
        "viewer should follow subject",
    );
    ok(
        repo.follow_user(&subject.id, &viewer.id).await,
        "subject should follow viewer back",
    );
    ok(
        repo.follow_user(&subject.id, &other.id).await,
        "subject should follow a third account",
    );
    ok(
        repo.set_user_signature(&subject.id, "watching everything")
            .await,
        "signature should be stored",
    );

    let stats = ok(
        repo.follow_profile_stats(&subject.id, Some(&viewer.id))
            .await,
        "profile stats should be readable",
    );
    assert_eq!(stats.signature, "watching everything");
    assert_eq!(stats.following_count, 2);
    assert_eq!(stats.follower_count, 1);
    assert!(stats.viewer_following, "viewer follows the subject");
    assert!(stats.following_viewer, "subject follows the viewer back");

    let anonymous = ok(
        repo.follow_profile_stats(&subject.id, None).await,
        "profile stats should be readable without a viewer",
    );
    assert!(
        !anonymous.viewer_following && !anonymous.following_viewer,
        "a signed-out reader has no relationship to report"
    );
    assert_eq!(anonymous.following_count, 2);
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_follow_profile_stats_and_listings_exclude_deleted_accounts() {
    let (_container, pool) = create_test_pool().await;
    let repo = UserRepository::new(pool.clone());
    let subject = create_user(&repo, "follow_deleted_subject").await;
    let live = create_user(&repo, "follow_deleted_live").await;
    let gone = create_user(&repo, "follow_deleted_gone").await;

    ok(
        repo.follow_user(&subject.id, &live.id).await,
        "subject should follow the live account",
    );
    ok(
        repo.follow_user(&subject.id, &gone.id).await,
        "subject should follow the account that leaves",
    );
    ok(
        repo.follow_user(&gone.id, &subject.id).await,
        "the account that leaves should follow back",
    );
    assert!(ok(
        repo.delete(&gone.id).await,
        "test account should be soft-deleted"
    ));

    let stats = ok(
        repo.follow_profile_stats(&subject.id, None).await,
        "profile stats should be readable",
    );
    assert_eq!(
        stats.following_count, 1,
        "a page must not claim follows the reader cannot open"
    );
    assert_eq!(stats.follower_count, 0);

    let (following, total) = ok(
        repo.list_following(&subject.id, first_page(), None).await,
        "following list should be readable",
    );
    assert_eq!(total, 1);
    assert_eq!(following.len(), 1);
    assert_eq!(following[0].user.id, live.id);
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_follow_listings_report_mutual_edges_and_filter_by_username() {
    let (_container, pool) = create_test_pool().await;
    let repo = UserRepository::new(pool.clone());
    let subject = create_user(&repo, "follow_list_subject").await;
    let mutual = create_user(&repo, "follow_list_mutual").await;
    let oneway = create_user(&repo, "follow_list_oneway").await;

    ok(
        repo.follow_user(&subject.id, &mutual.id).await,
        "subject should follow the mutual account",
    );
    ok(
        repo.follow_user(&mutual.id, &subject.id).await,
        "the mutual account should follow back",
    );
    ok(
        repo.follow_user(&subject.id, &oneway.id).await,
        "subject should follow the one-way account",
    );

    let (following, total) = ok(
        repo.list_following(&subject.id, first_page(), None).await,
        "following list should be readable",
    );
    assert_eq!(total, 2);
    let mutual_flag = |id: &UserId| {
        following
            .iter()
            .find(|followed| &followed.user.id == id)
            .is_some_and(|followed| followed.mutual)
    };
    assert!(mutual_flag(&mutual.id), "the reciprocated edge is mutual");
    assert!(
        !mutual_flag(&oneway.id),
        "the unreciprocated edge is not mutual"
    );

    let (followers, follower_total) = ok(
        repo.list_followers(&subject.id, first_page(), None).await,
        "follower list should be readable",
    );
    assert_eq!(follower_total, 1);
    assert_eq!(followers[0].user.id, mutual.id);
    assert!(followers[0].mutual);

    let (filtered, filtered_total) = ok(
        repo.list_following(&subject.id, first_page(), Some("oneway"))
            .await,
        "search should filter the following list",
    );
    assert_eq!(filtered_total, 1, "the count must respect the search too");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].user.id, oneway.id);
}

#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_set_user_signature_overwrites_and_clears() {
    let (_container, pool) = create_test_pool().await;
    let repo = UserRepository::new(pool.clone());
    let user = create_user(&repo, "follow_signature_owner").await;

    let empty = ok(
        repo.follow_profile_stats(&user.id, None).await,
        "profile stats should be readable before a signature exists",
    );
    assert_eq!(
        empty.signature, "",
        "an account without a signature reads as empty, not missing"
    );

    assert_eq!(
        ok(
            repo.set_user_signature(&user.id, "first").await,
            "signature should be stored"
        ),
        "first"
    );
    assert_eq!(
        ok(
            repo.set_user_signature(&user.id, "second").await,
            "signature should be overwritten"
        ),
        "second"
    );
    assert_eq!(
        ok(
            repo.set_user_signature(&user.id, "").await,
            "signature should be clearable"
        ),
        ""
    );
    let cleared = ok(
        repo.follow_profile_stats(&user.id, None).await,
        "profile stats should be readable after clearing",
    );
    assert_eq!(cleared.signature, "");
}
