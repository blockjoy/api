mod setup;

use api::{auth::FindableById, models};

#[tokio::test]
async fn can_create_key_file() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let node = tester.node().await;
    let req = models::NewNodeKeyFile {
        name: "my-key.txt",
        content:
            "asödlfasdf asdfjaöskdjfalsdjföasjdf afa sdffasdfasldfjasödfj asdföalksdföalskdjfa",
        node_id: node.id,
    };
    let mut conn = tester.conn().await;
    let file = req.create(&mut conn).await?;

    assert_eq!(file.name, "my-key.txt");

    Ok(())
}

#[tokio::test]
async fn cannot_create_key_file_for_unknown_node() {
    let tester = setup::Tester::new().await;
    let req = models::NewNodeKeyFile {
        name: "my-key.txt",
        content:
            "asödlfasdf asdfjaöskdjfalsdjföasjdf afa sdffasdfasldfjasödfj asdföalksdföalskdjfa",
        node_id: uuid::Uuid::new_v4(),
    };

    let mut conn = tester.conn().await;
    req.create(&mut conn).await.unwrap_err();
}

#[tokio::test]
async fn deletes_key_file_if_node_is_deleted() -> anyhow::Result<()> {
    let tester = setup::Tester::new().await;
    let node = tester.node().await;
    let req = models::NewNodeKeyFile {
        name: "my-key.txt",
        content:
            "asödlfasdf asdfjaöskdjfalsdjföasjdf afa sdffasdfasldfjasödfj asdföalksdföalskdjfa",
        node_id: node.id,
    };
    let mut conn = tester.conn().await;
    let file = req.create(&mut conn).await?;

    assert_eq!(file.name, "my-key.txt");

    models::Node::delete(node.id, &mut conn).await?;

    models::Node::find_by_id(node.id, &mut conn)
        .await
        .unwrap_err();

    Ok(())
}
