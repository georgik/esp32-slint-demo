#![no_std]
#![no_main]

use alloc::rc::Rc;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::panic::PanicInfo;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::{Duration as EspDuration, Instant};
use esp_println::println;
extern crate alloc;

// Import traits for rand_chacha.
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};
use slint::{Model, VecModel};
use spin::Mutex;
use core::cell::RefCell;
use slint::SharedString;
slint::include_modules!(); // This includes the compiled Slint UI file, which exports MainWindow.
// slint::slint! {
//     export { MainWindow } from "ui/pexeso_game.slint";
// }
// ----------------------------------------------------------------------
// Data Structures for the Game
// ----------------------------------------------------------------------

// Use &'static str for Level so that the constant initializer works.
#[derive(Clone)]
struct Level {
    level_name: &'static str,
    chain_length: usize, // e.g., 2 for pairs
    total_cards: usize,  // must be divisible by chain_length
}

#[derive(Clone)]
struct GameCard {
    card_id: String, // used to identify matching cards
    face: String,    // visual representation (e.g., an emoji)
    state: String,   // "hidden", "selected", or "solved"
}

// Global game state.
struct GameState {
    current_level: Level,
    board: Vec<GameCard>,
    selected_indices: Vec<usize>,
}

impl GameState {
    // Generate a shuffled board.
    fn generate_board(&mut self) {
        let faces = vec!["A", "B", "C", "D", "E", "F"];
        let groups = self.current_level.total_cards / self.current_level.chain_length;
        let mut cards: Vec<GameCard> = Vec::new();
        for i in 0..groups {
            let face = faces[i % faces.len()].to_string();
            for _ in 0..self.current_level.chain_length {
                cards.push(GameCard {
                    card_id: face.clone(),
                    face: face.clone(),
                    state: "hidden".into(),
                });
            }
        }
        // Shuffle cards using a Fisher–Yates algorithm with rand_chacha.
        let len = cards.len();
        let mut rng = ChaCha8Rng::from_seed([0u8; 32]);
        for i in 0..len {
            let j = i + (rng.next_u32() as usize % (len - i));
            cards.swap(i, j);
        }
        self.board = cards;
    }

    // Process a card selection.
    fn select_card(&mut self, index: usize) {
        if self.board[index].state != "hidden" {
            return;
        }
        self.board[index].state = "selected".into();
        self.selected_indices.push(index);
        if self.selected_indices.len() == self.current_level.chain_length {
            let first_id = &self.board[self.selected_indices[0]].card_id;
            let all_match = self
                .selected_indices
                .iter()
                .all(|&i| &self.board[i].card_id == first_id);
            if all_match {
                for &i in &self.selected_indices {
                    self.board[i].state = "solved".into();
                }
            } else {
                let selected = self.selected_indices.clone();
                // Schedule a flip-back after 1000ms using slint::Timer::single_shot.
                slint::Timer::single_shot(core::time::Duration::from_millis(1000), move || {
                    GAME_STATE.lock().borrow_mut().board.iter_mut().enumerate().for_each(|(i, card)| {
                        if selected.contains(&i) {
                            card.state = "hidden".into();
                        }
                    });
                    update_board_model();
                });
            }
            self.selected_indices.clear();
            update_board_model();
        }
    }
}

// ----------------------------------------------------------------------
// Global Game State Setup
// ----------------------------------------------------------------------
// Use a spin::Mutex (since we're in a no_std, single-threaded environment).
static GAME_STATE: Mutex<RefCell<GameState>> = Mutex::new(RefCell::new(GameState {
    current_level: Level {
        level_name: "Level 1",
        chain_length: 2,
        total_cards: 6, // 3 pairs
    },
    board: Vec::new(),
    selected_indices: Vec::new(),
}));

// The board model exposed to Slint.
// The UI expects each board entry to be a tuple:
// (SharedString, SharedString, SharedString, SharedString)
// corresponding to (card_id, face, state, level_name).
static mut BOARD_MODEL: Option<Rc<slint::VecModel<(SharedString, SharedString, SharedString, SharedString)>>> = None;

// Update the board model from the global game state.
fn update_board_model() {
    unsafe {
        if let Some(board_model) = &BOARD_MODEL {
            let mut new_vec: Vec<(SharedString, SharedString, SharedString, SharedString)> = Vec::new();
            let gs = GAME_STATE.lock();
            let state = gs.borrow();
            for card in state.board.iter() {
                new_vec.push((
                    SharedString::from(card.card_id.clone()),
                    SharedString::from(card.face.clone()),
                    SharedString::from(card.state.clone()),
                    SharedString::from(state.current_level.level_name),
                ));
            }
            board_model.set_vec(new_vec);
        }
    }
}

#[main]
fn main() -> ! {
    println!("Starting Pexeso Game");
    mcu_board_support::init();

    {
        let mut gs = GAME_STATE.lock();
        gs.borrow_mut().generate_board();
    }

    let board_model = unsafe {
        BOARD_MODEL = Some(Rc::new(slint::VecModel::default()));
        BOARD_MODEL.as_ref().unwrap().clone()
    };

    let level_model: Rc<dyn slint::Model<Data = (SharedString,)>> =
        Rc::new(slint::VecModel::from(vec![(SharedString::from("1"),)]));

    let main_window = MainWindow::new().unwrap();
    let mut level_data: Vec<LevelData> = main_window.get_level_model().iter().collect();
    // main_window.set_current_view(SharedString::from("level_selector"));
    main_window.set_current_view(SharedString::from("game_play"));

    // Run the UI.
    main_window.run().unwrap();

    loop {
        let delay_start = Instant::now();
        while delay_start.elapsed() < EspDuration::from_millis(500) {}
    }
}
