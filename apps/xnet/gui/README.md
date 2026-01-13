# xnet-gui — GPUI Frontend Architecture

A modular, maintainable GPUI application for managing the XMTP Network Testing Framework.

## Architecture Overview

This application follows a **Module-Based Architecture** that balances code organization, reusability, and simplicity. The design prioritizes DRY principles while maintaining clarity and ease of navigation.

```
┌────────────────────────────────────────────────────────┐
│                      main.rs                           │
│                  (Application entry)                   │
└────────────────────────┬───────────────────────────────┘
                         │
                         ▼
┌────────────────────────────────────────────────────────┐
│                  views/root.rs                         │
│              (Layout orchestration)                    │
│  ┌──────────────────────────────────────────────────┐ │
│  │ Responsibilities:                                │ │
│  │  • Manages application state                     │ │
│  │  • Coordinates action handlers                   │ │
│  │  • Delegates rendering to view modules           │ │
│  │  • Handles button click events                   │ │
│  └──────────────────────────────────────────────────┘ │
└──┬────────────────┬─────────────────┬──────────────┬──┘
   │                │                 │              │
   ▼                ▼                 ▼              ▼
┌──────┐      ┌──────────┐     ┌──────────┐   ┌─────────┐
│Header│      │Services  │     │  Nodes   │   │   Log   │
│View  │      │Panel     │     │  Panel   │   │  Panel  │
└──┬───┘      └────┬─────┘     └────┬─────┘   └────┬────┘
   │               │                │              │
   │  Uses:        │  Uses:         │  Uses:       │  Uses:
   │  • badges     │  • panels      │  • panels    │  • panels
   │               │  • list_items  │  • badges    │
   │               │                │              │
   └───────────────┴────────────────┴──────────────┘
                         │
                         ▼
         ┌───────────────────────────────────┐
         │      ui/ (Helper functions)       │
         │  • buttons.rs                     │
         │  • panels.rs                      │
         │  • badges.rs                      │
         └───────────────────────────────────┘
                         │
                         ▼
         ┌───────────────────────────────────┐
         │     actions/ + state/ + theme/    │
         │  (Business logic and data)        │
         └───────────────────────────────────┘
```

## Directory Structure

```
gui/
├── src/
│   ├── main.rs                    # Application entry point
│   ├── prelude.rs                 # Common imports (gpui + theme + ui)
│   │
│   ├── state.rs                   # Application state models
│   │   └── AppState, NetworkStatus, ServiceInfo, NodeInfo
│   │
│   ├── theme.rs                   # Color palette and spacing constants
│   │   └── bg_*, text_*, accent_*, btn_*, spacing_*, radius_*
│   │
│   ├── actions.rs                 # Business logic for async operations
│   │   └── execute_up(), execute_down(), execute_delete(), execute_add_node()
│   │
│   ├── ui/                        # Reusable UI helper functions
│   │   ├── mod.rs                 # Re-exports all helpers
│   │   ├── buttons.rs             # Button helpers (primary, danger, warning, success)
│   │   ├── panels.rs              # Panel containers and list items
│   │   └── badges.rs              # Status badges and dots
│   │
│   └── views/                     # View rendering modules
│       ├── mod.rs                 # Re-exports all views
│       ├── root.rs                # RootView (main orchestrator)
│       ├── header.rs              # Header with app title and status badge
│       ├── toolbar.rs             # Toolbar (currently unused, buttons in root)
│       ├── panels.rs              # Services and nodes panels
│       └── log.rs                 # Log panel and error bar
│
└── README.md                      # This file
```

## Module Responsibilities

### 🎯 `main.rs`
- Application entry point
- Window setup and configuration
- Initializes RootView

### 🎨 `theme.rs`
- Color palette (Catppuccin Mocha)
- Spacing constants
- Border radius constants
- Single source of truth for visual design

### 📊 `state.rs`
- Application state models
- `AppState`: Central state container
- `NetworkStatus`: Lifecycle enum
- `ServiceInfo`, `NodeInfo`: Data models

### ⚙️ `actions.rs`
- Business logic for async operations
- `execute_up()`, `execute_down()`, `execute_delete()`, `execute_add_node()`
- Pure async functions returning `Result<T, String>`
- Separated from view logic for testability

### 🧩 `ui/` (Helper Functions)
Reusable UI building blocks that return GPUI elements:

#### `buttons.rs`
```rust
// Helper functions for common button patterns
make_button(label, bg, disabled) -> impl IntoElement
primary_button(label, disabled) -> impl IntoElement
danger_button(label, disabled) -> impl IntoElement
warning_button(label, disabled) -> impl IntoElement
success_button(label, disabled) -> impl IntoElement
```

#### `panels.rs`
```rust
// Panel containers and list items
panel_container() -> Div  // Returns configurable div
panel_title(text, color) -> impl IntoElement
empty_state(text) -> impl IntoElement
list_item_row(dot_color, content) -> impl IntoElement
```

#### `badges.rs`
```rust
// Status indicators
status_badge(color, text) -> impl IntoElement
status_dot(color) -> impl IntoElement
```

### 👁️ `views/` (Rendering Modules)
Pure rendering functions that compose UI helpers:

#### `header.rs`
```rust
render_header(status: NetworkStatus) -> impl IntoElement
```
Renders the app title and status badge.

#### `panels.rs`
```rust
render_services_panel(services: &[ServiceInfo]) -> impl IntoElement
render_nodes_panel(nodes: &[NodeInfo]) -> impl IntoElement
```
Renders panels with lists of services or nodes.

#### `log.rs`
```rust
render_log_panel(log_lines: &[Arc<str>]) -> impl IntoElement
render_error_bar(last_error: &Option<String>) -> impl IntoElement
```
Renders log display and error notifications.

#### `root.rs`
```rust
pub struct RootView {
    state: AppState,
    busy: bool,
}
```
- Owns application state
- Coordinates action handlers (action_up, action_down, etc.)
- Delegates rendering to view modules
- Handles button click events (requires `cx.listener()`)

### 📦 `prelude.rs`
Common imports for convenience:
```rust
use crate::prelude::*;
// Provides: gpui::prelude::*, px, App, Context, Window, theme, ui
```

## Design Principles

### 1. **DRY (Don't Repeat Yourself)**
- UI patterns extracted into helper functions
- Colors and spacing centralized in `theme.rs`
- Business logic separated into `actions.rs`

### 2. **Separation of Concerns**
- **State**: `state.rs` contains only data models
- **Business Logic**: `actions.rs` contains async operations
- **Presentation**: `ui/` and `views/` handle rendering
- **Coordination**: `root.rs` orchestrates everything

### 3. **GPUI Idioms**
- Pure functions for stateless rendering
- `cx.notify()` for state updates
- `cx.spawn()` for async operations
- `WeakEntity` to avoid memory leaks

### 4. **Discoverability**
- Logical module organization
- Clear naming conventions
- Comprehensive documentation
- Re-exports in `mod.rs` files

## Adding New Features

### Example: Adding a "Restart" Button

#### 1. Add business logic (`actions.rs`):
```rust
pub async fn execute_restart() -> Result<(), String> {
    execute_down().await?;
    execute_up().await?;
    Ok(())
}
```

#### 2. Add action handler (`views/root.rs`):
```rust
fn action_restart(&mut self, cx: &mut Context<Self>) {
    if self.busy {
        return;
    }
    self.busy = true;
    self.state.network_status = NetworkStatus::Starting;
    self.state.push_log("Restarting services…");
    cx.notify();

    cx.spawn(async |this, cx| {
        let result = actions::execute_restart().await;
        cx.update(|cx| {
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(()) => {
                        view.state.network_status = NetworkStatus::Running;
                        view.state.push_log("Services restarted.");
                        view.state.services = actions::populate_services();
                    }
                    Err(msg) => {
                        view.state.network_status = NetworkStatus::Error;
                        view.state.last_error = Some(msg.clone());
                        view.state.push_log(msg);
                    }
                }
                cx.notify();
            });
        }).ok();
    }).detach();
}
```

#### 3. Add button to toolbar (`views/root.rs` in `render_toolbar`):
```rust
.child(self.make_clickable_button(
    "btn-restart",
    "Restart",
    theme::accent_mauve(),
    disabled,
    cx,
    |view, _, _, cx| view.action_restart(cx),
))
```

### Example: Adding a New Panel

#### 1. Create view module (`views/metrics_panel.rs`):
```rust
use gpui::prelude::*;
use crate::{theme, ui};

pub fn render_metrics_panel(cpu: f32, memory: f32) -> impl IntoElement {
    ui::panel_container()
        .w(px(200.0))
        .child(ui::panel_title("Metrics", theme::accent_yellow()))
        .child(
            div()
                .text_color(theme::text_primary())
                .child(format!("CPU: {:.1}%", cpu))
        )
        .child(
            div()
                .text_color(theme::text_primary())
                .child(format!("Memory: {:.1}%", memory))
        )
}
```

#### 2. Register module (`views/mod.rs`):
```rust
pub mod metrics_panel;
```

#### 3. Add to layout (`views/root.rs` in `render_panels`):
```rust
.child(views::metrics_panel::render_metrics_panel(
    self.state.cpu_usage,
    self.state.memory_usage,
))
```

### Example: Adding a Reusable Component

#### 1. Add to `ui/` module (`ui/cards.rs`):
```rust
use gpui::prelude::*;
use crate::theme;

pub fn info_card(title: &str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .bg(theme::bg_surface())
        .rounded(px(8.0))
        .p(px(12.0))
        .gap(px(4.0))
        .child(
            div()
                .text_color(theme::text_muted())
                .text_xs()
                .child(title)
        )
        .child(
            div()
                .text_color(theme::text_primary())
                .text_lg()
                .child(value)
        )
}
```

#### 2. Register in `ui/mod.rs`:
```rust
pub mod cards;
pub use cards::*;
```

#### 3. Use anywhere:
```rust
use crate::ui;

ui::info_card("Total Nodes", &format!("{}", node_count))
```

## Common Patterns

### Pattern: Conditional Rendering
```rust
.when(condition, |div| div.bg(theme::accent_green()))
```

### Pattern: Optional Child
```rust
.when_some(optional_value, |div, value| {
    div.child(render_value(value))
})
```

### Pattern: Iterating Over Items
```rust
let mut panel = ui::panel_container();
for item in items {
    panel = panel.child(render_item(item));
}
panel
```

### Pattern: Async Action
```rust
cx.spawn(async |this: WeakEntity<Self>, cx: &mut AsyncApp| {
    let result = some_async_operation().await;
    cx.update(|cx| {
        let _ = this.update(cx, |view, cx| {
            // Update state
            view.state.field = result;
            cx.notify();  // Trigger re-render
        });
    }).ok();
}).detach();
```

## Testing Strategy

### Unit Tests
Test individual functions in isolation:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_populate_services() {
        let services = actions::populate_services();
        assert_eq!(services.len(), 11);
        assert!(services.iter().any(|s| s.name == "toxiproxy"));
    }
}
```

### Integration Tests
Test GPUI components (requires headless mode):
```rust
#[gpui::test]
async fn test_root_view_initialization() {
    let app = gpui::TestApp::new();
    let view = app.new_view(|_| RootView::new());
    assert_eq!(view.read().state.network_status, NetworkStatus::Stopped);
}
```

## Performance Considerations

### Render Optimization
- **Minimize state mutations**: Only call `cx.notify()` when state actually changes
- **Use `WeakEntity`**: Prevents memory leaks in async operations
- **Avoid expensive computations in render**: Pre-compute in action handlers

### Memory Management
- **Log rolling**: Logs are limited to 200 lines (see `state.rs`)
- **Weak references**: Async tasks use `WeakEntity` to allow GC
- **Clone minimization**: Use `Arc<str>` for shared strings

## Migration Path

This architecture is designed for growth. When the app needs more scalability:

### → Component-Based Architecture
Extract stateless components using `RenderOnce`:
```rust
struct Button {
    label: SharedString,
    variant: ButtonVariant,
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        ui::make_button(self.label, self.variant.color(), false)
    }
}
```

### → Feature-Based Architecture
Organize by domain when features become complex:
```
src/
├── features/
│   ├── network/      # Network management feature
│   ├── nodes/        # Node management feature
│   └── logs/         # Logging feature
└── shared/           # Shared components and theme
```

## Resources

### GPUI Documentation
- Official docs: https://docs.rs/gpui
- Zed source code: https://github.com/zed-industries/zed
- GPUI ownership guide: https://zed.dev/blog/gpui-ownership

### Project Files
- See `main.rs` for application structure
- See `theme.rs` for color palette
- See `ui/` modules for reusable helpers
- See `views/` modules for rendering logic

---

**Architecture Version**: 1.0 (Module-Based)
**Last Updated**: 2025-01-31
**GPUI Version**: 0.2
