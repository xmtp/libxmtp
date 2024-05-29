use crate::{builder::ClientBuilder, Client};
use futures::future::join_all;
use tokio::time::Instant;
use xmtp_api_grpc::Client as GrpcClient;
use xmtp_cryptography::utils::generate_local_wallet;

async fn create_client() -> Client<GrpcClient> {
    ClientBuilder::new_test_client(&generate_local_wallet()).await
}

async fn create_inboxes(num_accounts: u32) -> Vec<String> {
    let all_accounts = (0..num_accounts).collect::<Vec<u32>>();
    let chunks = all_accounts.chunks(50);

    let mut inbox_ids: Vec<String> = vec![];
    for chunk in chunks {
        inbox_ids.extend(
            join_all(chunk.iter().map(|_| async {
                let client = create_client().await;
                client.inbox_id()
            }))
            .await,
        )
    }

    inbox_ids
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn add_10_members() {
    let client = create_client().await;
    let other_inboxes = create_inboxes(10).await;
    let group = client.create_group(None).unwrap();

    log::info!("Starting to add members");
    // Time the function
    let start = Instant::now();
    group
        .add_members_by_inbox_id(&client, other_inboxes)
        .await
        .unwrap();
    let duration = start.elapsed();

    println!("Time to add 10 members to empty group: {:?}", duration);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn add_100_members() {
    let client = create_client().await;
    let other_inboxes = create_inboxes(100).await;
    let group = client.create_group(None).unwrap();

    // Time the function
    let start = Instant::now();
    group
        .add_members_by_inbox_id(&client, other_inboxes)
        .await
        .unwrap();
    let duration = start.elapsed();

    println!("Time to add 100 members to empty group: {:?}", duration);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn add_1_member_to_100_member_group() {
    let client = create_client().await;
    let other_inboxes = create_inboxes(100).await;
    let group = client.create_group(None).unwrap();
    group
        .add_members_by_inbox_id(&client, other_inboxes)
        .await
        .unwrap();

    let to_add = create_inboxes(1).await;
    // Time the function
    let start = Instant::now();
    group
        .add_members_by_inbox_id(&client, to_add)
        .await
        .unwrap();
    let duration = start.elapsed();

    println!("Time to add 1 member to 100 member group: {:?}", duration);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 10)]
async fn add_100_members_to_100_member_group() {
    let client = create_client().await;
    let other_inboxes = create_inboxes(100).await;
    let group = client.create_group(None).unwrap();
    group
        .add_members_by_inbox_id(&client, other_inboxes)
        .await
        .unwrap();

    let to_add = create_inboxes(100).await;
    // Time the function
    let start = Instant::now();
    group
        .add_members_by_inbox_id(&client, to_add)
        .await
        .unwrap();
    let duration = start.elapsed();

    println!(
        "Time to add 100 members to 100 member group: {:?}",
        duration
    );
}
