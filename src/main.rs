#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release
#![allow(rustdoc::missing_crate_level_docs)]

use eframe::egui::{CentralPanel, Context, ScrollArea, RichText};
use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};
use arboard::Clipboard;
use std::path::PathBuf;
use std::fs;

use serde::{Serialize, Deserialize};

const MAX_HISTORY: usize = 1000;

fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    
    let options = eframe::NativeOptions::default();
    
    eframe::run_native(
        "CopyCat - Clipboard Manager", 
        options, 
        Box::new(|cc| Ok(Box::new(CopyCatApp::new(cc))))
    )
}

#[derive(Serialize, Deserialize)]
struct ClipboardEntry {
    id: u64,
    content: String,
    timestamp: u64,
    favorite: bool,
}

impl ClipboardEntry {
    fn new(content: String) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        Self {
            id: timestamp,
            content,
            timestamp,
            favorite: false,
        }
    }
    
    fn formatted_time(&self) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        let diff = now - self.timestamp;
        
        if diff < 60 {
            format!("{}s ago", diff)
        } else if diff < 3600 {
            format!("{}m ago", diff / 60)
        } else if diff < 86400 {
            format!("{}h ago", diff / 3600)
        } else {
            format!("{}d ago", diff / 86400)
        }
    }
}

struct CopyCatApp {
    clipboard_history: VecDeque<ClipboardEntry>,
    clipboard: Clipboard,
    search_query: String,
    last_clipboard_content: String,
    filter_favorites: bool,
    selected_entry: Option<u64>,
    poll_interval_ms: u64,
    last_poll: u64,
    history_file: PathBuf,
}

impl CopyCatApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Initialize clipboard
        let clipboard = Clipboard::new().unwrap_or_else(|e| {
            eprintln!("Failed to initialize clipboard: {}", e);
            std::process::exit(1);
        });
        
        // Define file path for history (adjust as needed)
        let history_file = PathBuf::from("clipboard_history.json");
        let clipboard_history = Self::load_history(&history_file);
        
        Self {
            clipboard_history,
            clipboard,
            search_query: String::new(),
            last_clipboard_content: String::new(),
            filter_favorites: false,
            selected_entry: None,
            poll_interval_ms: 500, // Poll every 500ms
            last_poll: 0,
            history_file,
        }
    }
    
    /// Load clipboard history from disk. If the file doesn't exist or fails to parse, returns an empty VecDeque.
    fn load_history(path: &PathBuf) -> VecDeque<ClipboardEntry> {
        if path.exists() {
            match fs::read_to_string(path) {
                Ok(content) => {
                    if let Ok(history) = serde_json::from_str::<VecDeque<ClipboardEntry>>(&content) {
                        return history;
                    } else {
                        eprintln!("Failed to parse clipboard history, starting with empty history.");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to read history file: {}", e);
                }
            }
        }
        VecDeque::with_capacity(MAX_HISTORY)
    }
    
    /// Save the current clipboard history to disk.
    fn save_history(&self) {
        if let Ok(json) = serde_json::to_string(&self.clipboard_history) {
            if let Err(e) = fs::write(&self.history_file, json) {
                eprintln!("Failed to write history file: {}", e);
            }
        }
    }
    
    fn poll_clipboard(&mut self) {
        if let Ok(text) = self.clipboard.get_text() {
            if !text.is_empty() && text != self.last_clipboard_content {
                self.last_clipboard_content = text.clone();
                self.add_to_history(text);
            }
        }
    }
    
    fn add_to_history(&mut self, content: String) {
        // Don't add duplicates
        if self.clipboard_history.iter().any(|entry| entry.content == content) {
            return;
        }
        
        let entry = ClipboardEntry::new(content);
        
        if self.clipboard_history.len() >= MAX_HISTORY {
            // Remove oldest non-favorite entry
            if let Some(index) = self.clipboard_history.iter()
                .position(|entry| !entry.favorite) {
                self.clipboard_history.remove(index);
            } else {
                // All entries are favorites, remove oldest
                self.clipboard_history.pop_back();
            }
        }
        
        self.clipboard_history.push_front(entry);
        self.save_history();
    }
    
    fn copy_to_clipboard(&mut self, content: &str) {
        if let Err(e) = self.clipboard.set_text(content.to_string()) {
            eprintln!("Failed to copy to clipboard: {}", e);
        }
    }
    
    fn filtered_history(&self) -> Vec<&ClipboardEntry> {
        self.clipboard_history.iter()
            .filter(|entry| {
                if self.filter_favorites && !entry.favorite {
                    return false;
                }
                
                if !self.search_query.is_empty() {
                    return entry.content.to_lowercase().contains(&self.search_query.to_lowercase());
                }
                
                true
            })
            .collect()
    }
    
    fn toggle_favorite(&mut self, id: u64) {
        if let Some(entry) = self.clipboard_history.iter_mut().find(|e| e.id == id) {
            entry.favorite = !entry.favorite;
            self.save_history();
        }
    }
}

// Define action enum for deferred operations
enum Action {
    ToggleFavorite(u64),
    Select(u64, String),
    Copy(String),
    Delete(u64),
}

// Define a struct to hold all the data we need from an entry
struct EntryDisplayData {
    id: u64,
    content: String,
    is_selected: bool,
    is_favorite: bool,
    display_text: String,
}

impl eframe::App for CopyCatApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Poll clipboard at specified interval
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
            
        if now - self.last_poll > self.poll_interval_ms {
            self.poll_clipboard();
            self.last_poll = now;
        }
        
        // Request repaint to keep polling
        ctx.request_repaint_after(std::time::Duration::from_millis(self.poll_interval_ms));

        CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("CopyCat Clipboard Manager");
            });
            
            ui.add_space(10.0);
            
            // Search and filters
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut self.search_query);
                ui.checkbox(&mut self.filter_favorites, "Favorites only");
            });
            
            ui.add_space(5.0);
            
            // Clipboard history
            ui.label(RichText::new("Clipboard History").strong());
            
            // Prepare all the data we need from filtered_history
            let mut entries_data = Vec::new();
            {
                let filtered_history = self.filtered_history();
                let filtered_is_empty = filtered_history.is_empty();
                
                if filtered_is_empty {
                    ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
                        ui.label("No clipboard entries found");
                    });
                } else {
                    for entry in filtered_history {
                        let mut content_display = entry.content.clone();
                        if content_display.len() > 50 {
                            content_display = format!("{}...", &content_display[..47]);
                        }
                        
                        entries_data.push(EntryDisplayData {
                            id: entry.id,
                            content: entry.content.clone(),
                            is_selected: Some(entry.id) == self.selected_entry,
                            is_favorite: entry.favorite,
                            display_text: format!("{} ({})", content_display, entry.formatted_time()),
                        });
                    }
                }
            } // filtered_history goes out of scope here
            
            // Now we can collect actions and process them without borrowing issues
            let mut actions = Vec::new();
            
            if !entries_data.is_empty() {
                ScrollArea::vertical().max_height(500.0).show(ui, |ui| {
                    for entry_data in &entries_data {
                        ui.horizontal(|ui| {
                            // Toggle favorite button
                            if ui.selectable_label(entry_data.is_favorite, "★").clicked() {
                                actions.push(Action::ToggleFavorite(entry_data.id));
                            }
                            
                            // Display and select entry
                            let response = ui.selectable_label(
                                entry_data.is_selected, 
                                &entry_data.display_text
                            );
                            
                            if response.clicked() {
                                actions.push(Action::Select(entry_data.id, entry_data.content.clone()));
                            }
                            
                            // Context menu
                            response.context_menu(|ui| {
                                if ui.button("Copy").clicked() {
                                    actions.push(Action::Copy(entry_data.content.clone()));
                                    ui.close_menu();
                                }
                                
                                if ui.button("Delete").clicked() {
                                    actions.push(Action::Delete(entry_data.id));
                                    ui.close_menu();
                                }
                                
                                let fav_text = if entry_data.is_favorite { "Unmark favorite" } else { "Mark favorite" };
                                if ui.button(fav_text).clicked() {
                                    actions.push(Action::ToggleFavorite(entry_data.id));
                                    ui.close_menu();
                                }
                            });
                        });
                    }
                });
            }
            
            // Process all actions
            for action in actions {
                match action {
                    Action::ToggleFavorite(id) => self.toggle_favorite(id),
                    Action::Select(id, content) => {
                        self.selected_entry = Some(id);
                        self.copy_to_clipboard(&content);
                    },
                    Action::Copy(content) => self.copy_to_clipboard(&content),
                    Action::Delete(id) => {
                        if let Some(index) = self.clipboard_history.iter()
                            .position(|e| e.id == id) {
                            self.clipboard_history.remove(index);
                            self.save_history();
                        }
                    },
                }
            }
            
            ui.add_space(10.0);
            
            // Buttons
            ui.horizontal(|ui| {
                if ui.button("Clear All").clicked() {
                    self.clipboard_history.clear();
                    self.save_history();
                }
                
                if ui.button("Clear Non-Favorites").clicked() {
                    self.clipboard_history.retain(|entry| entry.favorite);
                    self.save_history();
                }
            });
            
            // Status bar
            ui.add_space(5.0);
            ui.separator();
            ui.label(format!("Total entries: {}/{}", self.clipboard_history.len(), MAX_HISTORY));
        });
    }
}
