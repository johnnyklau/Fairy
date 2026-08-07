# Visual Spec — Companion App

## Reference

Style is based on "Fairy" from Zenless Zone Zero: a circular eye icon + dark speech-bubble popups.

## Eye icon (always visible)

- Concentric rings design (per uploaded reference image): outer glow ring → white ring → colored ring → solid colored center → small white highlight dot (top-right of pupil)
- Fixed size, fixed position on screen for v1
- Color stays constant blue (#378ADD) at all times — idle and all reminder types
- Glow intensity (not color) signals state change: soft glow when idle, brighter/stronger glow on reminder trigger
- Idle animation: subtle scale pulse (breathing), continuous loop
- On reminder trigger: brief glow intensity increase, hold, then popup appears, glow returns to idle level after dismiss

## Notification popup (on reminder trigger)

- Dark semi-transparent bubble (rgba(20,20,20,0.92)), rounded corners (~14px)
- Positioned adjacent to the eye icon, tail/notch pointing toward it
- Small circular badge (mini eye icon) overlapping the bubble's edge nearest the eye
- Text: bold italic, ~14px, light gray/white (#f2f2f2), left-aligned, sentence case
- One line preferred, two lines max
- No buttons, no icons inside text — visual only
- Auto-dismiss after a few seconds (fade out)
- No sound in v1

## Idle bark popups (optional, low priority)

- Same bubble style as reminders
- Triggered after long idle periods, rare frequency
- Flavor text only, not actionable

## Motion notes

- All transitions should be smooth (ease in/out), no linear snaps
- Popup: fade + slight upward slide on appear, fade on dismiss
- Eye state change: cross-fade color, not hard cut

## Explicitly out of scope for v1 visuals

- Character animation (walking, arms, face beyond the eye)
- Multiple popups stacked/queued simultaneously
- Theming/skins
