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
use core::cell::RefCell;
use critical_section::Mutex;
use critical_section::with;
use slint::SharedString;
use core::time::Duration;
use slint::Timer;
slint::include_modules!(); // This includes the compiled Slint UI file, which exports MainWindow.

// Cache of the Slint-defined card_set so we can reuse their images
static mut DEFAULT_CARDS: Option<Vec<Card>> = None;

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
        // Available images for cards
        let mut available = vec![
            "cherry", "cheese", "carrot", "rose",
            "barrel", "ghost", "sun", "butterfly",
            "cloud", "dwarf",
        ];
        // Use ChaCha8Rng for reproducible randomness
        let mut rng = ChaCha8Rng::from_seed([0u8; 32]);
        // Shuffle available images
        let avail_len = available.len();
        for i in 0..avail_len {
            let j = i + (rng.next_u32() as usize % (avail_len - i));
            available.swap(i, j);
        }
        // Build exactly two pairs
        let pair_count = self.current_level.total_cards / self.current_level.chain_length;
        let mut cards: Vec<GameCard> = Vec::new();
        for face_str in available.iter().take(pair_count) {
            let face = face_str.to_string();
            for _ in 0..self.current_level.chain_length {
                cards.push(GameCard {
                    card_id: face.clone(),
                    face: face.clone(),
                    state: "hidden".into(),
                });
            }
        }
        // Shuffle the final 4 cards
        let len = cards.len();
        for i in 0..len {
            let j = i + (rng.next_u32() as usize % (len - i));
            cards.swap(i, j);
        }
        self.board = cards;
    }

    // Process a card selection. Returns true if a flip occurred, false otherwise.
    fn select_card(&mut self, index: usize) -> bool {
        if self.board[index].state != "hidden" {
            return false;
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
                // mismatch: flip-back will be handled externally
            }
            self.selected_indices.clear();
        }
        true
    }
}

// ----------------------------------------------------------------------
// Global Game State Setup
// ----------------------------------------------------------------------
// Use a critical-section Mutex around a RefCell for safe static mutation.
static GAME_STATE: Mutex<RefCell<GameState>> = Mutex::new(RefCell::new(GameState {
    current_level: Level {
        level_name: "Level 1",
        chain_length: 2, // pairs
        total_cards: 4,  // 2 pairs for 2×2
    },
    board: Vec::new(),
    selected_indices: Vec::new(),
}));

// The board model exposed to Slint.
// The UI expects each board entry to be a tuple:
// (SharedString, SharedString, SharedString, SharedString)
// corresponding to (card_id, face, state, level_name).
static mut BOARD_MODEL: Option<Rc<slint::VecModel<Card>>> = None;

// Update the board model from the global game state.
fn update_board_model() {

    // Reuse the Slint Card definitions for images
    let default_cards = unsafe { DEFAULT_CARDS.as_ref().unwrap() };

    unsafe {
        if let Some(board_model) = &BOARD_MODEL {
            let mut new_vec: Vec<Card> = Vec::new();
            // Read the game state inside a critical section
            with(|cs| {
                let state_ref = GAME_STATE.borrow(cs).borrow();
                for (i, card) in state_ref.board.iter().enumerate() {
                    // Compute 2×2 positions; adjust spacing if needed
                    let x = if i % 2 == 0 { 0 } else { (480 - 30) / 2 + 10 } as i32;
                    let y = if i < 2 { 80 } else { 80 + (480 - 100) / 2 + 10 } as i32;
                    // Map the GameCard into the Slint-generated Card
                    new_vec.push(Card {
                        id: SharedString::from(card.card_id.clone()),
                        image: default_cards[i % default_cards.len()].image.clone(),
                        is_face_up: card.state != "hidden",
                        values: Rc::new(VecModel::default()).into(),
                        x,
                        y,
                    });
                }
            });
            board_model.set_vec(new_vec);
        }
    }
}

#[main]
fn main() -> ! {
    println!("Starting Pexeso Game");
    mcu_board_support::init();

    {
        with(|cs| {
            GAME_STATE.borrow(cs).borrow_mut().generate_board();
        });
    }

    let board_model = unsafe {
        BOARD_MODEL = Some(Rc::new(slint::VecModel::default()));
        BOARD_MODEL.as_ref().unwrap().clone()
    };

    // Create the Slint window
    let main_window = MainWindow::new().unwrap();

    // Grab the UI's built‑in card_set and stash it for reuse
    let card_set_into: Vec<Card> = main_window.get_card_set().iter().collect();
    unsafe { DEFAULT_CARDS = Some(card_set_into); }

    // Now initialize the Slint board with our generated cards
    update_board_model();

    // Construct a Slint Board and push it into the window
    let board = Board { cards: board_model.into() };
    main_window.set_board_model(board);

    // Create the level model as a SharedVector-like model using VecModel;
    // here we wrap a tuple in a VecModel and then convert it to a ModelRc.
    let level_model: Rc<dyn slint::Model<Data = (SharedString,)>> =
        Rc::new(slint::VecModel::from(vec![(SharedString::from("1"),)]));

    // let mut level_data: Vec<LevelData> = main_window.get_level_model().iter().collect();
    // level_data.clear();
    // level_data.extend(
    //     vec![
    //         LevelData {
    //             level_name: SharedString::from("1"),
    //             locked: false
    //         },
    //         LevelData {
    //             level_name: SharedString::from("2"),
    //             locked: true
    //         },
    //         LevelData {
    //             level_name: SharedString::from("3"),
    //             locked: true
    //         },
    //     ],
    // );


    // let level_model = Rc::new(VecModel::from(level_data));
    //
    // main_window.set_level_model(level_model.clone().into());

    // main_window.set_level_model(level_model.into());
    main_window.set_current_view(SharedString::from("level_selector"));

    // When the UI reports a card flip, run our game logic in Rust
    let mw_weak = main_window.as_weak();
    main_window.on_flip_card(move |card_index| {
        // 1) Attempt to flip via GameState inside critical section
        let did_flip = {
            let mut flipped = false;
            with(|cs| {
                let mut gs = GAME_STATE.borrow(cs).borrow_mut();
                flipped = gs.select_card(card_index as usize);
            });
            flipped
        };
        if !did_flip {
            return;
        }
        // 2) Immediately refresh the UI
        update_board_model();

        // 3) After a mismatch (two cards still "selected"), flip them back after a delay
        // Find mismatched selections and flip them back after 800ms
        let mut mismatch = Vec::new();
        with(|cs| {
            let state_ref = GAME_STATE.borrow(cs).borrow();
            for (i, card) in state_ref.board.iter().enumerate() {
                if card.state == "selected" {
                    mismatch.push(i);
                }
            }
        });
        if mismatch.len() == 2 {
            // Schedule flip-back in 800ms
            let mismatch_clone = mismatch.clone();
            Timer::single_shot(Duration::from_millis(800), move || {
                with(|cs| {
                    let mut gs = GAME_STATE.borrow(cs).borrow_mut();
                    for &i in &mismatch_clone {
                        gs.board[i].state = "hidden".into();
                    }
                });
                update_board_model();
            });
        }
    });

    // let mw_weak = main_window.as_weak();
    // main_window.on_level_selected(move |_level_index| {
    //     if let Some(mw) = mw_weak.upgrade() {
    //         {
    //             let mut gs = GAME_STATE.lock();
    //             println!("Starting {}", gs.borrow().current_level.level_name);
    //             gs.borrow_mut().generate_board();
    //         }
    //         update_board_model();
    //         mw.set_current_view(SharedString::from("game_board"));
    //     }
    // });
    //
    //
    // let mw_weak = main_window.as_weak();
    // main_window.on_card_selected(move |card_index| {
    //     if let Some(mw) = mw_weak.upgrade() {
    //         {
    //             let mut gs = GAME_STATE.lock();
    //             gs.borrow_mut().select_card(card_index as usize);
    //         }
    //         update_board_model();
    //     }
    // });


    // Run the UI.
    main_window.run().unwrap();

    loop {
        let delay_start = Instant::now();
        while delay_start.elapsed() < EspDuration::from_millis(500) {}
    }
}
