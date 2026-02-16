use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, InputMode};
use crate::db::Status;

pub fn render(app: &App, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),    // kanban
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    render_header(app, frame, chunks[0]);
    render_kanban(app, frame, chunks[1]);
    render_footer(app, frame, chunks[2]);

    if app.input_mode == InputMode::NewSession {
        render_new_session_popup(app, frame);
    }
}

fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let header = Paragraph::new(format!("Project: {} ({})", app.project.name, app.project.path))
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, area);
}

fn render_kanban(app: &App, frame: &mut Frame, area: Rect) {
    let statuses = Status::all();
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Ratio(1, statuses.len() as u32); statuses.len()])
        .split(area);

    for (col_idx, status) in statuses.iter().enumerate() {
        let sessions = app.sessions_by_status(*status);
        let is_selected_column = col_idx == app.selected_column;

        let border_style = if is_selected_column {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let items: Vec<ListItem> = sessions
            .iter()
            .enumerate()
            .map(|(row_idx, session)| {
                let is_selected = is_selected_column && row_idx == app.selected_row;
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let content = if let Some(ticket) = &session.ticket_id {
                    format!("[{}] {}", ticket, session.name)
                } else {
                    session.name.clone()
                };

                ListItem::new(Line::from(Span::styled(content, style)))
            })
            .collect();

        let title = format!(" {} ({}) ", status.label(), sessions.len());
        let list = List::new(items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        );

        frame.render_widget(list, columns[col_idx]);
    }
}

fn render_footer(_app: &App, frame: &mut Frame, area: Rect) {
    let help = "q: quit | n: new | hjkl: navigate | m: move | d: delete | r: refresh";
    let footer = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, area);
}

fn render_new_session_popup(app: &App, frame: &mut Frame) {
    let area = centered_rect(50, 20, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" New Session ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input = Paragraph::new(app.input_buffer.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::BOTTOM).title("Name"));

    frame.render_widget(input, inner);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
