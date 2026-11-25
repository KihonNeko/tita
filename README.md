# tita

Learn Morse code by ear, in your terminal.

tita plays a character, you type what you heard. Characters are sent at full
speed with long pauses between them, so you learn the *sound* of each letter
instead of counting dots and dashes. New characters unlock once you reach 90 %
accuracy on the current ones.

The interface is in French.

## Install

Download the archive for your system from the
[releases page](https://github.com/KihonNeko/tita/releases), extract it and run
`tita` from a terminal.

| System  | Sound                                                        |
|---------|--------------------------------------------------------------|
| Linux   | needs `pw-cat` (PipeWire) or `aplay` (alsa-utils) installed  |
| macOS   | works out of the box                                         |
| Windows | works out of the box                                         |

On macOS the binary is not signed: the first time, allow it with
`xattr -d com.apple.quarantine tita`.

Or build from source with [Rust](https://rustup.rs):

```
cargo install --git https://github.com/KihonNeko/tita
```

## Use

```
tita                      # start the trainer
tita play "CQ CQ" --wpm 20   # play some text in Morse
tita show "SOS"           # print the dot/dash patterns
```

In the menu: `↑` `↓` to choose, `Enter` to start, `Esc` to stop a session or
go back, `q` to quit.

Start with **Lettres**. You begin with E and T; the rest follows as you
progress. Short sessions, several times a day, work best.

## Modes

| Mode             | What it does                                                                 |
|------------------|------------------------------------------------------------------------------|
| Lettres          | One character plays, you press its key. Mistakes are replayed and come back. |
| Lettres sans fin | Same, with no time limit and no pause: trains reaction time.                 |
| Découverte       | Listen to the unlocked characters with their pattern shown.                  |
| Koch             | Groups of 5 characters at full speed; you copy, it is graded at the end.     |
| Flux             | A continuous stream that never waits for you; speed adapts to your accuracy. |
| Mots             | Common words (French or English) made only of the letters you know.          |
| Copie de tête    | The whole word plays first, then you type it from memory.                    |
| Radio            | Callsigns, Q codes and abbreviations, simulated contacts.                    |
| Émission         | Send Morse with the keyboard; tita decodes your rhythm.                      |
| Statistiques     | Accuracy and reaction time per character, most frequent confusions.          |
| Réglages         | Speeds, tone, volume, session length, learning order.                        |

## Characters

62 characters: letters, digits, punctuation, É À È Ç, and special signals.
Special signals are typed with one key each:

| Key | Signal | Meaning                |
|-----|--------|------------------------|
| `+` | AR     | end of message         |
| `=` | BT     | break                  |
| `(` | KN     | over to you only       |
| `&` | AS     | wait                   |
| `<` | KA     | start of transmission  |
| `>` | SK     | end of contact         |
| `#` | HH     | error (8 dots)         |
| `*` | SOS    | distress               |

Two learning orders are available in **Réglages**: lessons by groups of letters
(E T, then I M S O, ...) or the classic Koch order (K M U R E S ...).

## Settings

- **Vitesse des caractères**: how fast each character is sent (default 20 wpm).
- **Vitesse effective**: overall speed, set by the pauses (default 8 wpm).
  Raise it as you improve.
- If you already know some letters, unlock them by hand in **Réglages**.

Settings and progress are stored in your user config and data folders
(`~/.config/tita` and `~/.local/share/tita` on Linux).
