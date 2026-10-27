//! What the system says of its network reaches QML through
//! `NetworkInformation` and Rust through `ljos::system`.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use ljos::system::{
    NetworkStatus, Reachability, TransportMedium, network_status, on_network_change,
    report_network_status,
};

const MAIN: &str = r#"
import QtQuick
import QtNetwork
Item {
    property bool online: NetworkInformation.reachability === NetworkInformation.Reachability.Online
    property int medium: NetworkInformation.transportMedium
}
"#;

fn wifi() -> NetworkStatus {
    NetworkStatus {
        reachability: Reachability::Online,
        transport_medium: TransportMedium::WiFi,
        ..NetworkStatus::default()
    }
}

#[test]
fn what_the_host_says_reaches_qml_and_rust() {
    let engine = ljos::engine();
    let root = common::create_main(&engine, MAIN);
    assert!(!engine.read_by_name(root, "online").to_boolean());
    assert_eq!(network_status(&engine), NetworkStatus::default());

    report_network_status(&engine, wifi());
    assert!(engine.read_by_name(root, "online").to_boolean());
    assert_eq!(engine.read_by_name(root, "medium").to_number(), 3.0);
    assert_eq!(network_status(&engine), wifi());
}

#[test]
fn what_was_said_before_qml_asked_is_what_it_reads() {
    let engine = ljos::engine();
    report_network_status(&engine, wifi());
    let root = common::create_main(&engine, MAIN);
    assert!(engine.read_by_name(root, "online").to_boolean());
}

#[test]
fn rust_hears_each_change_once() {
    let engine = ljos::engine();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let into = Rc::clone(&heard);
    let watch =
        on_network_change(&engine, move |_, s| into.borrow_mut().push(s.reachability)).unwrap();
    assert!(heard.borrow().is_empty());

    report_network_status(&engine, wifi());
    report_network_status(&engine, wifi());
    let offline = NetworkStatus {
        reachability: Reachability::Disconnected,
        ..NetworkStatus::default()
    };
    report_network_status(&engine, offline);
    assert_eq!(
        *heard.borrow(),
        [Reachability::Online, Reachability::Disconnected]
    );

    watch.stop(&engine);
    report_network_status(&engine, wifi());
    assert_eq!(heard.borrow().len(), 2);
}
