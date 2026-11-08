//! Files another `octa <file>` start handed over (`octa::single_instance`).

use eframe::egui;

use super::state::OctaApp;

impl OctaApp {
    /// Open handed-over files like command-line files and bring the window
    /// forward. Windows' foreground rules and Wayland's activation protocol
    /// may refuse the focus; the taskbar entry then flashes instead.
    pub(crate) fn drain_instance_handover(&mut self, ctx: &egui::Context) {
        if std::mem::take(&mut self.attention_pending)
            && ctx.input(|i| i.viewport().focused) == Some(false)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                egui::UserAttentionType::Informational,
            ));
        }
        // Drain into a local first: answering an RPC borrows `self`
        // mutably, which the receiver borrow forbids.
        let deliveries: Vec<octa::single_instance::Delivery> = match &self.instance {
            Some(l) => {
                let mut v = Vec::new();
                while let Ok(d) = l.rx.try_recv() {
                    v.push(d);
                }
                v
            }
            None => return,
        };
        let mut paths = Vec::new();
        for delivery in deliveries {
            match delivery {
                octa::single_instance::Delivery::Files(batch) => paths.extend(batch),
                // Handled right here on the UI thread so the handler sees
                // real tab state; the serving thread is blocked on our reply.
                octa::single_instance::Delivery::Rpc { line, reply } => {
                    let _ = reply.send(crate::app::rpc::handle(self, &line));
                }
            }
        }
        if paths.is_empty() {
            return;
        }
        self.enqueue_open_files(paths);
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.attention_pending = true;
        ctx.request_repaint();
    }
}
