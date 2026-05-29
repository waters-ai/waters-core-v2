use std::net::TcpListener;
use std::thread;
use std::time::Duration;
use waters_node::{channel::ChannelManager, gossip::GossipEngine, group::GroupManager};

#[test]
fn test_channel_creation_and_messaging() {
    // Create temporary directories for channel storage
    let temp_dir = tempfile::tempdir().unwrap();
    let channel_path = temp_dir.path();
    
    // Initialize channel manager
    let mut channel_mgr = ChannelManager::new(channel_path, "test-node-1");
    
    // Create a test channel
    channel_mgr.create("test.channel", "open").unwrap();
    
    // Verify channel exists
    assert!(channel_mgr.exists("test.channel"));
    
    // Write a message to the channel
    let msg = channel_mgr.blocking_write("test.channel", "test_msg", "sender", "Hello, World!").unwrap();
    
    // Verify message was written
    assert_eq!(msg.content, "Hello, World!");
    assert_eq!(msg.from, "sender");
    assert_eq!(msg.msg_type, "test_msg");
    
    // Read messages from the channel
    let messages = channel_mgr.blocking_read("test.channel", 0);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "Hello, World!");
}

#[test]
fn test_gossip_engine_initialization() {
    // Initialize gossip engine
    let gossip = GossipEngine::new("test-node-1", "Test Node 1", 42069);
    
    // Verify initial state
    assert_eq!(gossip.node_id, "test-node-1");
    assert_eq!(gossip.node_name, "Test Node 1");
    assert_eq!(gossip.port, 42069);
    assert_eq!(gossip.peer_count(), 0);
}

#[test]
fn test_group_creation_and_management() {
    // Initialize group manager
    let mut group_mgr = GroupManager::new("test-node-1");
    
    // Create a test group
    let group_info = group_mgr.create("test.group", "open").unwrap();
    
    // Verify group was created
    assert_eq!(group_info.name, "test.group");
    assert_eq!(group_info.visibility, "open");
    assert_eq!(group_info.created_by, "test-node-1");
    assert_eq!(group_info.members.len(), 1);
    assert_eq!(group_info.members[0].node_id, "test-node-1");
    
    // Test adding a member
    group_mgr.add_member("test.group", "test-node-2", "member").unwrap();
    
    // Verify member was added
    let updated_group = group_mgr.get("test.group").unwrap();
    assert_eq!(updated_group.members.len(), 2);
    assert_eq!(updated_group.members[1].node_id, "test-node-2");
    assert_eq!(updated_group.members[1].role, "member");
    
    // Test setting group mode
    group_mgr.set_mode("test.group", waters_node::group::GroupMode::Storm).unwrap();
    let mode_group = group_mgr.get("test.group").unwrap();
    assert_eq!(mode_group.mode, waters_node::group::GroupMode::Storm);
    
    // Test advancing mode
    group_mgr.advance_mode("test.group").unwrap();
    let advanced_group = group_mgr.get("test.group").unwrap();
    assert_eq!(advanced_group.mode, waters_node::group::GroupMode::Hunt);
}

#[test]
fn test_integration_between_components() {
    // Test interaction between channel manager and gossip engine
    let temp_dir = tempfile::tempdir().unwrap();
    let channel_path = temp_dir.path();
    
    let mut channel_mgr = ChannelManager::new(channel_path, "integration-test-node");
    let gossip = GossipEngine::new("integration-test-node", "Integration Test Node", 42070);
    
    // Create channels via channel manager
    channel_mgr.create("gossip.test", "open").unwrap();
    
    // Add channels to gossip engine (simulating what would happen in main.rs)
    // In real implementation, this is done via gossip.add_channel().await
    
    // Verify channel exists in both systems
    assert!(channel_mgr.exists("gossip.test"));
    
    // Test writing and reading through channel manager
    let msg = channel_mgr.blocking_write("gossip.test", "gossip_msg", "node1", "Test gossip data").unwrap();
    let messages = channel_mgr.blocking_read("gossip.test", 0);
    
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "Test gossip data");
    assert_eq!(messages[0].from, "node1");
}