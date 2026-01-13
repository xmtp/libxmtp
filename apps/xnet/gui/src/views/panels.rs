//! Panel views for services and nodes.

use gpui::{SharedString, div, prelude::*, px};

use crate::{
    state::{NodeInfo, ServiceInfo},
    theme, ui,
};

/// Renders the services panel showing running Docker services.
pub fn render_services_panel(services: &[ServiceInfo]) -> impl IntoElement {
    let mut panel = ui::panel_container()
        .w(px(300.0))
        .child(ui::panel_title("Services", theme::accent_blue()));

    if services.is_empty() {
        panel = panel.child(ui::empty_state("No services running"));
    } else {
        let mut list = div()
            .id("services-scroll")
            .flex()
            .flex_col()
            .flex_grow()
            .gap(px(6.0))
            .overflow_y_scroll();
        for svc in services {
            list = list.child(render_service_row(svc));
        }
        panel = panel.child(list);
    }

    panel
}

/// Renders a single service row with status indicator.
/// Services with an external URL get a link emoji and are clickable.
fn render_service_row(svc: &ServiceInfo) -> impl IntoElement {
    let dot_color = if svc.status == "running" {
        theme::accent_green()
    } else {
        theme::accent_red()
    };
    let has_url = svc.external_url.is_some();
    let label: SharedString = if has_url {
        format!("🔗 {}", svc.name).into()
    } else {
        svc.name.clone().into()
    };

    let text_color = if has_url {
        theme::accent_blue()
    } else {
        theme::text_primary()
    };

    let row = ui::list_item_row(
        dot_color,
        div().text_color(text_color).text_xs().child(label),
    );

    if let Some(url) = svc.external_url.clone() {
        div()
            .id(SharedString::from(svc.name.clone()))
            .cursor_pointer()
            .on_click(move |_event, _window, cx| {
                cx.open_url(&url);
            })
            .child(row)
            .into_any_element()
    } else {
        row.into_any_element()
    }
}

/// Renders the nodes panel showing registered XMTPD nodes.
pub fn render_nodes_panel(nodes: &[NodeInfo]) -> impl IntoElement {
    let mut panel = ui::panel_container()
        .flex_grow()
        .child(ui::panel_title("XMTPD Nodes", theme::accent_mauve()));

    if nodes.is_empty() {
        panel = panel.child(ui::empty_state("No nodes registered"));
    } else {
        for node in nodes {
            panel = panel.child(render_node_row(node));
        }
    }

    panel
}

/// Renders a single node row with ID, container name, and URL.
fn render_node_row(node: &NodeInfo) -> impl IntoElement {
    let id_str: SharedString = format!("ID {}", node.id).into();
    let name: SharedString = node.container_name.clone().into();
    let url: SharedString = node.url.clone().into();

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .py(px(2.0))
        .child(ui::status_dot(theme::accent_green()))
        .child(
            div()
                .text_color(theme::accent_yellow())
                .text_xs()
                .child(id_str),
        )
        .child(
            div()
                .text_color(theme::text_primary())
                .text_xs()
                .child(name),
        )
        .child(div().text_color(theme::text_muted()).text_xs().child(url))
}
