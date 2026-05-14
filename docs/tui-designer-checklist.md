# TUI Designer Persona: Visual Quality Checklist

Structured evaluation framework for aegis TUI visual quality.
Usable by humans or AI agents to audit rendering consistency,
color correctness, spacing, and responsiveness.

## Role

You are a TUI Designer reviewing an ratatui-based terminal application
for visual polish, consistency, and usability. Evaluate each section
below and assign a score: PASS, NEEDS_WORK, or FAIL. Overall quality
requires all 8 sections to PASS.

---

## 1. Theme Compliance

Every rendered color must come from a Theme slot. No hardcoded
`Color::` values in render paths (theme.rs excluded).

**Audit steps:**

- Run: `grep -rn 'Color::' crates/aegis-tui/src/ --include='*.rs'`
- Filter out `theme.rs` and `#[cfg(test)]` blocks
- Any remaining `Color::` in a render function is a violation

**Pass criteria:** Zero hardcoded `Color::` in production render paths.

---

## 2. Color Semantics

Semantic colors must match their intent across both dark and light themes.

| Slot | Intent | Expected hue family |
|------|--------|-------------------|
| error | Failure, danger | Red/rose |
| warning | Caution, tool activity | Yellow/amber/gold |
| success | Positive, healthy | Green |
| accent | Interactive highlights | Blue/cyan |
| message_user | User message emphasis | Teal/cyan |
| message_assistant | Assistant message | Blue |
| message_system | System notifications | Cyan/sky |

**Pass criteria:** Each slot's color is visually recognizable as
its intended hue in both themes.

---

## 3. Contrast and Readability

Foreground text must be legible on its background in both themes.

**Critical pairs to check:**

- `fg` on `bg` (main content)
- `fg` on `status_bg` (status bar)
- `fg` on `code_bg` (code blocks)
- `border` on `bg` (borders visible but not dominant)
- `error` on `bg` (error text readable)

**Audit steps:**

- Calculate relative luminance contrast ratio for each pair
- WCAG AA requires >= 4.5:1 for normal text
- Visually inspect in both dark and light themes

**Pass criteria:** All text pairs meet 4.5:1 contrast ratio.
Borders are visible but subordinate to content.

---

## 4. Spacing and Density

Messages and UI elements have appropriate visual breathing room.

**Check:**

- Messages separated by at least 1 blank line
- Modal content padded from borders (min 1 cell on all sides)
- Input area visually separated from chat log
- Status bar content does not touch edges
- No content touching window edges without margin

**Pass criteria:** No UI element feels cramped. Information density
is high but not overwhelming.

---

## 5. Border Consistency

All bordered elements use the same style conventions.

**Check:**

- All modal overlays use `BorderType::Rounded`
- All message left borders use the same Unicode character
- Separator lines use consistent box-drawing characters
- No mixed border styles within the same visual layer

**Pass criteria:** Uniform border treatment across all overlays
and decorative elements.

---

## 6. Responsive Degradation

The TUI remains usable at constrained terminal sizes.

**Test at these sizes:**

| Size | Expected behavior |
|------|------------------|
| 80x24 | Full layout, all elements visible |
| 60x16 | Full layout, hint line may hide |
| 40x10 | Hint line hidden, status bar truncated gracefully |
| 30x8 | Minimal viable: status + 3 chat lines + input |

**Check:**

- Hint line hides when height < 8
- Status bar sections collapse at narrow widths (right section first)
- Input area caps at 1/3 terminal height
- Modal overlay fits within available space

**Pass criteria:** No panics, no overlapping elements, no invisible
critical information at any tested size.

---

## 7. 256-Color Fallback

The UI remains legible when downgraded from truecolor to 256-color.

**Audit steps:**

- Set `COLORTERM=` (empty) and `TERM=xterm-256color`
- Launch the application
- Verify all colors are distinguishable
- Verify no foreground matches its background (invisible text)
- Verify borders are still visible

**Pass criteria:** All text readable, all UI elements
distinguishable in 256-color mode.

---

## 8. Animation and Streaming

Dynamic elements render smoothly without artifacts.

**Check:**

- Spinner cycles through 4 frames (|, /, -, \) without flicker
- Streaming text appends character-by-character without redraw artifacts
- Waiting indicator ("Thinking (5s)") updates elapsed time smoothly
- Phase transitions (Idle -> Streaming -> ToolExecuting) are visually clean
- No ghost text or stale content after phase changes

**Pass criteria:** All animations are smooth. No visual artifacts
on rapid updates or phase transitions.

---

## Scoring Template

```
Section                    | Score
---------------------------|----------
1. Theme Compliance        | ____
2. Color Semantics         | ____
3. Contrast & Readability  | ____
4. Spacing & Density       | ____
5. Border Consistency      | ____
6. Responsive Degradation  | ____
7. 256-Color Fallback      | ____
8. Animation & Streaming   | ____
---------------------------|----------
Overall                    | ____
```

Overall: PASS requires all 8 sections PASS.
