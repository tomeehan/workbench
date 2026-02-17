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
    render_kanban_footer(app, frame, chunks[2]);

    if app.input_mode == InputMode::NewSession {
        render_input_popup(app, frame, "New Session");
    } else if app.input_mode == InputMode::EditSession {
        render_edit_session_popup(app, frame);
    } else if app.input_mode == InputMode::MoveSession {
        render_move_popup(frame);
    } else if app.input_mode == InputMode::ConfirmDelete {
        render_confirm_delete_popup(app, frame);
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

    let help = "q/Esc: back | n: new | e: edit | d: delete | v: toggle visible | jk: nav | JK: reorder";
    let footer = Paragraph::new(help).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[2]);

    match app.input_mode {
        InputMode::NewFieldName => render_field_popup(app, frame, "New Field", "Name", &app.new_field_name),
        InputMode::NewFieldDesc => render_field_popup(app, frame, "New Field", "Description", &app.new_field_desc),
        InputMode::EditFieldName => render_field_popup(app, frame, "Edit Field", "Name", &app.new_field_name),
        InputMode::EditFieldDesc => render_field_popup(app, frame, "Edit Field", "Description", &app.new_field_desc),
        InputMode::ConfirmDeleteField => render_confirm_delete_field_popup(app, frame),
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
            } else if !field.visible {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            let visibility = if field.visible { "👁" } else { "  " };
            let text = if field.description.is_empty() {
                format!("{} {}", visibility, field.name)
            } else {
                format!("{} {} - {}", visibility, field.name, field.description)
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

        // Render column header
        let title = format!(" {} ({}) ", status.label(), sessions.len());
        let column_block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner_area = column_block.inner(columns[col_idx]);
        frame.render_widget(column_block, columns[col_idx]);

        // Calculate card heights and render each card
        let visible_fields = app.fields.iter().filter(|f| f.visible).count();
        let card_height = 4 + visible_fields as u16; // base height + visible fields
        let mut y_offset = 0u16;

        for (row_idx, session) in sessions.iter().enumerate() {
            if y_offset >= inner_area.height {
                break; // No more room
            }

            let card_area = Rect {
                x: inner_area.x,
                y: inner_area.y + y_offset,
                width: inner_area.width,
                height: card_height.min(inner_area.height - y_offset),
            };

            render_session_card(app, frame, session, is_selected_column, row_idx, card_area);
            y_offset += card_height;
        }
    }
}

fn render_session_card(app: &App, frame: &mut Frame, session: &Session, is_selected_column: bool, row_idx: usize, area: Rect) {
    let is_selected = is_selected_column && row_idx == app.selected_row;

    let border_style = if is_selected {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let name_style = if is_selected {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };

    let detail_style = Style::default().fg(Color::DarkGray);

    // Build card title with indicator
    let title = if app.is_waiting_for_input(session) {
        format!(" ? {} ", session.name)
    } else if app.has_active_terminal(session) {
        format!(" $ {} ", session.name)
    } else {
        format!(" {} ", session.name)
    };

    let title_style = if app.is_waiting_for_input(session) {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else if app.has_active_terminal(session) {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        name_style
    };

    let card_block = Block::default()
        .title(Span::styled(title, title_style))
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = card_block.inner(area);
    frame.render_widget(card_block, area);

    // Build card content
    let mut lines: Vec<Line> = Vec::new();

    // Branch name (if active terminal)
    if let Some(ref tmux_name) = session.tmux_window {
        if app.active_tmux_sessions.contains(tmux_name) {
            if let Some(branch) = tmux::get_git_branch(tmux_name) {
                lines.push(Line::from(vec![
                    Span::styled("⎇ ", Style::default().fg(Color::Blue)),
                    Span::styled(branch, Style::default().fg(Color::Blue)),
                ]));
            }
        }
    }

    // Custom field values (only visible fields)
    for field in app.fields.iter().filter(|f| f.visible) {
        let value = app.db.get_session_field_value(session.id, field.id).unwrap_or_default();
        if !value.is_empty() {
            let is_url = value.starts_with("http://") || value.starts_with("https://");
            // Truncate long values to fit card width
            let max_len = 25;
            let display_value: String = if value.chars().count() > max_len {
                format!("{}…", value.chars().take(max_len).collect::<String>())
            } else {
                value
            };
            let value_style = if is_url {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{}: ", field.name), detail_style),
                Span::styled(display_value, value_style),
            ]));
        }
    }

    let content = Paragraph::new(lines);
    frame.render_widget(content, inner);
}

fn render_kanban_footer(app: &App, frame: &mut Frame, area: Rect) {
    let text = if let Some(ref msg) = app.status_message {
        msg.clone()
    } else {
        "q: quit | n: new | e: edit | Space: peek | hjkl: nav | m: move | d: del | r: refresh | x: cleanup | s: settings | Enter: term".to_string()
    };
    let style = if app.status_message.is_some() {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let footer = Paragraph::new(text).style(style);
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
    use crate::app::EditMode;

    let num_fields = app.fields.len();
    let is_ai_mode = app.edit_mode == EditMode::AI;

    // In AI mode, add extra row for AI input
    let total_display_rows = if is_ai_mode { 2 + num_fields } else { 1 + num_fields };
    let popup_height = std::cmp::min(20 + (total_display_rows * 3) as u16, 80);
    let area = centered_rect(60, popup_height, frame.area());
    frame.render_widget(Clear, area);

    let mode_str = if app.ai_running {
        "AI ⏳ Running..."
    } else {
        match app.edit_mode {
            EditMode::Manual => "Manual",
            EditMode::AI => "AI ✨",
        }
    };
    let help = if app.ai_running {
        "Please wait..."
    } else if is_ai_mode {
        "Shift+Tab: mode, Enter: run AI"
    } else {
        "Shift+Tab: mode, Tab/↑↓: nav, Enter: save"
    };
    let title = format!(" Edit Session [{}] ({}) ", mode_str, help);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Create constraints for each row
    let constraints: Vec<Constraint> = (0..total_display_rows)
        .map(|_| Constraint::Length(3))
        .collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let mut row_offset = 0;

    // In AI mode, render AI input field first
    if is_ai_mode {
        let (ai_style, ai_title) = if app.ai_running {
            (Style::default().fg(Color::Yellow), "⏳ Running AI... please wait".to_string())
        } else if let Some(ref err) = app.ai_error {
            (Style::default().fg(Color::Red), format!("❌ Error: {}", err.chars().take(40).collect::<String>()))
        } else {
            (Style::default().fg(Color::Magenta), "✨ AI Prompt (describe what to fill)".to_string())
        };
        let ai_block = Block::default()
            .borders(Borders::BOTTOM)
            .title(ai_title)
            .border_style(ai_style);
        let ai_input = Paragraph::new(app.ai_input.as_str())
            .style(ai_style)
            .block(ai_block);
        if !rows.is_empty() {
            frame.render_widget(ai_input, rows[0]);
        }
        row_offset = 1;
    }

    // Render name field
    let name_row = row_offset;
    let name_selected = !is_ai_mode && app.edit_row == 0;
    let name_style = if is_ai_mode {
        Style::default().fg(Color::DarkGray) // Locked in AI mode
    } else if name_selected {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let name_value = if name_selected && !is_ai_mode {
        app.input_buffer.as_str()
    } else {
        app.edit_session_name.as_str()
    };
    let name_prefix = if is_ai_mode { "  " } else if name_selected { "> " } else { "  " };
    let name_block = Block::default()
        .borders(Borders::BOTTOM)
        .title(format!("{}Name", name_prefix))
        .border_style(name_style);
    let name_input = Paragraph::new(name_value)
        .style(name_style)
        .block(name_block);
    if name_row < rows.len() {
        frame.render_widget(name_input, rows[name_row]);
    }

    // Render custom fields
    for (i, field) in app.fields.iter().enumerate() {
        let row_idx = row_offset + 1 + i;
        if row_idx >= rows.len() {
            break;
        }
        let is_selected = !is_ai_mode && app.edit_row == i + 1;
        let style = if is_ai_mode {
            Style::default().fg(Color::DarkGray) // Locked in AI mode
        } else if is_selected {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let value = if is_selected && !is_ai_mode {
            app.input_buffer.as_str()
        } else {
            app.edit_field_values.get(i).map(|s| s.as_str()).unwrap_or("")
        };
        let prefix = if is_ai_mode { "  " } else if is_selected { "> " } else { "  " };
        let title = format!("{}{}", prefix, field.name);
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

fn render_confirm_delete_field_popup(app: &App, frame: &mut Frame) {
    let field_name = app.deleting_field_id
        .and_then(|id| app.fields.iter().find(|f| f.id == id))
        .map(|f| f.name.as_str())
        .unwrap_or("this field");

    let area = centered_rect(40, 20, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Delete Field ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = format!("Delete \"{}\"?\n\n(y)es / (n)o", field_name);
    let para = Paragraph::new(text)
        .style(Style::default().fg(Color::White))
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(para, inner);
}

fn render_confirm_delete_popup(app: &App, frame: &mut Frame) {
    let session_name = app.deleting_session_id
        .and_then(|id| app.sessions.iter().find(|s| s.id == id))
        .map(|s| s.name.as_str())
        .unwrap_or("this session");

    let area = centered_rect(40, 20, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Delete Session ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = format!("Delete \"{}\"?\n\n(y)es / (n)o", session_name);
    let para = Paragraph::new(text)
        .style(Style::default().fg(Color::White))
        .alignment(ratatui::layout::Alignment::Center);
    frame.render_widget(para, inner);
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
