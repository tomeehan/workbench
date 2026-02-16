use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, InputMode, View};
use crate::db::{Session, Status};
use crate::tmux;

pub fn render(app: &App, frame: &mut Frame) {
    match app.view {
        View::Kanban => render_kanban_view(app, frame),
        View::Settings => render_settings_view(app, frame),
    }
}

fn render_kanban_view(app: &App, frame: &mut Frame) {
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
    render_kanban_footer(frame, chunks[2]);

    if app.input_mode == InputMode::NewSession {
        render_input_popup(app, frame, "New Session");
    } else if app.input_mode == InputMode::EditSession {
        render_edit_session_popup(app, frame);
    } else if app.input_mode == InputMode::MoveSession {
        render_move_popup(frame);
    }

    if app.peek_active {
        render_peek_overlay(app, frame);
    }
}

fn render_settings_view(app: &App, frame: &mut Frame) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(0),    // fields list
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    let header = Paragraph::new("Settings: Custom Fields")
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(header, chunks[0]);

    render_fields_list(app, frame, chunks[1]);

    let help = "q/Esc: back | n: new field | e: edit | d: delete | jk: navigate | JK: reorder";
    let footer = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[2]);

    match app.input_mode {
        InputMode::NewFieldName => render_field_popup(app, frame, "New Field", "Name", &app.new_field_name),
        InputMode::NewFieldDesc => render_field_popup(app, frame, "New Field", "Description", &app.new_field_desc),
        InputMode::EditFieldName => render_field_popup(app, frame, "Edit Field", "Name", &app.new_field_name),
        InputMode::EditFieldDesc => render_field_popup(app, frame, "Edit Field", "Description", &app.new_field_desc),
        _ => {}
    }
}

fn render_fields_list(app: &App, frame: &mut Frame, area: Rect) {
    let items: Vec<ListItem> = app
        .fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let is_selected = idx == app.selected_field;
            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let text = if field.description.is_empty() {
                field.name.clone()
            } else {
                format!("{} - {}", field.name, field.description)
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let block = Block::default()
        .title(" Fields ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

fn render_field_popup(app: &App, frame: &mut Frame, title: &str, field_label: &str, value: &str) {
    let area = centered_rect(50, 30, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3)])
        .split(inner);

    // Show name field
    let name_style = if field_label == "Name" {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let name_value = if field_label == "Name" { value } else { &app.new_field_name };
    let name_input = Paragraph::new(name_value.to_string())
        .style(name_style)
        .block(Block::default().borders(Borders::BOTTOM).title("Name"));
    frame.render_widget(name_input, inner_chunks[0]);

    // Show description field
    let desc_style = if field_label == "Description" {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let desc_value = if field_label == "Description" { value } else { &app.new_field_desc };
    let desc_input = Paragraph::new(desc_value.to_string())
        .style(desc_style)
        .block(Block::default().borders(Borders::BOTTOM).title("Description"));
    frame.render_widget(desc_input, inner_chunks[1]);
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
                render_session_card(app, session, is_selected_column, row_idx)
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

fn render_session_card<'a>(app: &App, session: &Session, is_selected_column: bool, row_idx: usize) -> ListItem<'a> {
    let is_selected = is_selected_column && row_idx == app.selected_row;

    let name_style = if is_selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };

    let detail_style = if is_selected {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut lines: Vec<Line> = Vec::new();

    // Line 1: Status indicator + Name
    let mut name_spans = Vec::new();
    if app.is_waiting_for_input(session) {
        let indicator_style = if is_selected {
            Style::default().fg(Color::Yellow).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        };
        name_spans.push(Span::styled("? ", indicator_style));
    } else if app.has_active_terminal(session) {
        let indicator_style = if is_selected {
            Style::default().fg(Color::Green).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        };
        name_spans.push(Span::styled("$ ", indicator_style));
    }
    name_spans.push(Span::styled(session.name.clone(), name_style));
    lines.push(Line::from(name_spans));

    // Line 2: Branch name (if active terminal)
    if let Some(ref tmux_name) = session.tmux_window {
        if app.active_tmux_sessions.contains(tmux_name) {
            if let Some(branch) = tmux::get_git_branch(tmux_name) {
                let branch_style = if is_selected {
                    Style::default().fg(Color::Blue).bg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Blue)
                };
                lines.push(Line::from(vec![
                    Span::styled("  ⎇ ", branch_style),
                    Span::styled(branch, branch_style),
                ]));
            }
        }
    }

    // Lines 3+: Custom field values
    for field in &app.fields {
        let value = app.db.get_session_field_value(session.id, field.id).unwrap_or_default();
        if !value.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(format!("  {}: ", field.name), detail_style),
                Span::styled(value, detail_style),
            ]));
        }
    }

    // Add separator line for visual spacing
    lines.push(Line::from(""));

    ListItem::new(lines)
}

fn render_kanban_footer(frame: &mut Frame, area: Rect) {
    let help = "q: quit | n: new | e: edit | Space: peek | hjkl: navigate | m: move | d: delete | r: refresh | s: settings | Enter: terminal";
    let footer = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, area);
}

fn render_peek_overlay(app: &App, frame: &mut Frame) {
    let Some(session) = app.selected_session() else { return };
    let Some(ref tmux_name) = session.tmux_window else { return };

    let content = tmux::capture_pane_content(tmux_name)
        .unwrap_or_else(|| "(no content)".to_string());

    let area = centered_rect(80, 70, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(format!(" {} ", session.name))
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let para = Paragraph::new(content)
        .style(Style::default().fg(Color::White));
    frame.render_widget(para, inner);
}

fn render_input_popup(app: &App, frame: &mut Frame, title: &str) {
    let area = centered_rect(50, 20, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input = Paragraph::new(app.input_buffer.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::BOTTOM).title("Name"));

    frame.render_widget(input, inner);
}

fn render_edit_session_popup(app: &App, frame: &mut Frame) {
    let num_fields = app.fields.len();
    let total_rows = 1 + num_fields;
    let popup_height = std::cmp::min(20 + (num_fields * 3) as u16, 80);
    let area = centered_rect(60, popup_height, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Edit Session (Tab/↑↓ to navigate, Enter to save) ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Create constraints for each row
    let constraints: Vec<Constraint> = (0..total_rows)
        .map(|_| Constraint::Length(3))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    // Render name field (row 0)
    let name_selected = app.edit_row == 0;
    let name_style = if name_selected {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let name_value = if name_selected {
        app.input_buffer.as_str()
    } else {
        app.edit_session_name.as_str()
    };
    let name_block = Block::default()
        .borders(Borders::BOTTOM)
        .title(if name_selected { "> Name" } else { "  Name" })
        .border_style(name_style);
    let name_input = Paragraph::new(name_value)
        .style(name_style)
        .block(name_block);
    if !rows.is_empty() {
        frame.render_widget(name_input, rows[0]);
    }

    // Render custom fields
    for (i, field) in app.fields.iter().enumerate() {
        let row_idx = i + 1;
        if row_idx >= rows.len() {
            break;
        }
        let is_selected = app.edit_row == row_idx;
        let style = if is_selected {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let value = if is_selected {
            app.input_buffer.as_str()
        } else {
            app.edit_field_values.get(i).map(|s| s.as_str()).unwrap_or("")
        };
        let title = if is_selected {
            format!("> {}", field.name)
        } else {
            format!("  {}", field.name)
        };
        let field_block = Block::default()
            .borders(Borders::BOTTOM)
            .title(title)
            .border_style(style);
        let field_input = Paragraph::new(value)
            .style(style)
            .block(field_block);
        frame.render_widget(field_input, rows[row_idx]);
    }
}

fn render_move_popup(frame: &mut Frame) {
    let area = centered_rect(30, 25, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Move to ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let statuses = Status::all();
    let items: Vec<ListItem> = statuses
        .iter()
        .enumerate()
        .map(|(i, status)| {
            let text = format!("{}: {}", i + 1, status.label());
            ListItem::new(text).style(Style::default().fg(Color::White))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
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
