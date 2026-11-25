# tita

Learn Morse code by ear, in your terminal.

tita plays a character, you type what you heard. Characters are sent at full
speed with long pauses between them, so you learn the *sound* of each letter
instead of counting dots and dashes. New characters unlock once you reach 90 %
accuracy on the current ones.

The interface is available in English and French. It starts in your system
language; change it in **Settings → Interface language** (**Réglages → Langue de
l'interface**).

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

Start with **Letters**. You begin with E and T; the rest follows as you
progress. Short sessions, several times a day, work best.

## Modes

| Mode            | In French        | What it does                                                                 |
|-----------------|------------------|------------------------------------------------------------------------------|
| Letters         | Lettres          | One character plays, you press its key. Mistakes are replayed and come back. |
| Endless letters | Lettres sans fin | Same, with no time limit and no pause: trains reaction time.                 |
| Discover        | Découverte       | Listen to the unlocked characters with their pattern shown.                  |
| Koch            | Koch             | Groups of 5 characters at full speed; you copy, it is graded at the end.     |
| Stream          | Flux             | A continuous stream that never waits for you; speed adapts to your accuracy. |
| Words           | Mots             | Common words (French or English) made only of the letters you know.          |
| Head copy       | Copie de tête    | The whole word plays first, then you type it from memory.                    |
| Radio           | Radio            | Callsigns, Q codes and abbreviations, simulated contacts.                    |
| Sending         | Émission         | Send Morse with the keyboard; tita decodes your rhythm.                      |
| Statistics      | Statistiques     | Accuracy and reaction time per character, most frequent confusions.          |
| Settings        | Réglages         | Language, speeds, tone, volume, session length, learning order.              |

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

Two learning orders are available in **Settings**: lessons by groups of letters
(E T, then I M S O, ...) or the classic Koch order (K M U R E S ...).

## Settings

- **Interface language**: English or French.
- **Character speed**: how fast each character is sent (default 20 wpm).
- **Effective speed**: overall speed, set by the pauses (default 8 wpm).
  Raise it as you improve.
- **Word list language**: French or English words in the word modes.
- If you already know some letters, unlock them by hand at the bottom of the
  list.

Settings and progress are stored in your user config and data folders
(`~/.config/tita` and `~/.local/share/tita` on Linux).
