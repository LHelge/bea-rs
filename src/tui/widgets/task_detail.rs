use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use crate::graph::Graph;
use crate::task::Task;

use super::super::style::Theme;
use super::body::BodyWidget;
use super::dep_tree::DepTreeWidget;
use super::task_info::TaskInfoWidget;

/// Metrics fed back to App for scroll clamping.
pub(in crate::tui) struct DetailMetrics {
    pub content_height: u16,
    pub visible_height: u16,
}

pub(in crate::tui) struct TaskDetailWidget<'a> {
    task: Option<&'a Task>,
    graph: &'a Graph,
    task_map: &'a HashMap<String, Task>,
    scroll: u16,
    border_style: Style,
    metrics: &'a mut DetailMetrics,
    theme: &'a Theme,
}

impl<'a> TaskDetailWidget<'a> {
    pub fn new(
        task: Option<&'a Task>,
        graph: &'a Graph,
        task_map: &'a HashMap<String, Task>,
        scroll: u16,
        border_style: Style,
        metrics: &'a mut DetailMetrics,
        theme: &'a Theme,
    ) -> Self {
        Self {
            task,
            graph,
            task_map,
            scroll,
            border_style,
            metrics,
            theme,
        }
    }
}

impl TaskDetailWidget<'_> {
    pub fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .title(" Detail ")
            .borders(Borders::ALL)
            .border_style(self.border_style);

        let Some(task) = self.task else {
            Paragraph::new("No tasks found.")
                .block(block)
                .render(area, buf);
            self.metrics.content_height = 0;
            self.metrics.visible_height = area.height.saturating_sub(2);
            return;
        };

        let mut lines: Vec<Line> = Vec::new();

        // Sub-widget: task info (title + metadata)
        lines.extend(TaskInfoWidget::new(task, self.theme).lines());

        // Sub-widget: dependency tree
        lines.extend(DepTreeWidget::new(task, self.graph, self.task_map, self.theme).lines());

        // Sub-widget: body
        lines.extend(BodyWidget::new(&task.body, self.theme).lines());

        // Compute wrapped content height
        let inner_width = area.width.saturating_sub(2) as usize;
        let content_height = if inner_width == 0 {
            lines.len() as u16
        } else {
            lines
                .iter()
                .map(|line| {
                    let w = line.width();
                    if w <= inner_width {
                        1u16
                    } else {
                        (w as u16).div_ceil(inner_width as u16)
                    }
                })
                .sum()
        };
        let visible_height = area.height.saturating_sub(2);

        self.metrics.content_height = content_height;
        self.metrics.visible_height = visible_height;

        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .render(area, buf);
    }
}
