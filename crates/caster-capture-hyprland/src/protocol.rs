//! Generated client bindings for Hyprland's surface export protocol.

pub mod wlr_foreign_toplevel {
    use wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!(
            "../../native/protocols/wlr-foreign-toplevel-management-unstable-v1.xml"
        );
    }

    use self::__interfaces::*;
    wayland_scanner::generate_client_code!(
        "../../native/protocols/wlr-foreign-toplevel-management-unstable-v1.xml"
    );
}

pub mod hyprland_toplevel_export {
    use wayland_client;
    use wayland_client::protocol::*;

    pub use super::wlr_foreign_toplevel::zwlr_foreign_toplevel_handle_v1;

    pub mod __interfaces {
        use super::super::wlr_foreign_toplevel::__interfaces::{
            ZWLR_FOREIGN_TOPLEVEL_HANDLE_V1_INTERFACE, zwlr_foreign_toplevel_handle_v1_interface,
        };
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!(
            "../../native/protocols/hyprland-toplevel-export-v1.xml"
        );
    }

    use self::__interfaces::*;
    wayland_scanner::generate_client_code!(
        "../../native/protocols/hyprland-toplevel-export-v1.xml"
    );
}
