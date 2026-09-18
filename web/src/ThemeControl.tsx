/**
 * The theme switch at the foot of the sidebar: System, Paper or Ink
 * (spec 2026-09-17 §3).
 *
 * **A choice takes effect the moment it is made** — stored, then applied to
 * `<html>` — with nothing to confirm, because a theme is cheap to change back
 * and a preview that needed a Save would only be a second place for the two to
 * disagree. The pre-paint script in `index.html` applies the stored choice on
 * the next load, before this component exists.
 *
 * Native radios in a `fieldset`, so the group is one tab stop with arrow keys
 * between its three choices and a screen reader names it by its legend;
 * `shell.css` draws them as one segmented row.
 */
import { useState } from 'react'
import {
  applyTheme,
  loadTheme,
  saveTheme,
  THEME_CHOICES,
  type ThemeChoice,
} from './theme'

const LABELS: Record<ThemeChoice, string> = {
  system: 'System',
  paper: 'Paper',
  ink: 'Ink',
}

export function ThemeControl() {
  const [choice, setChoice] = useState<ThemeChoice>(loadTheme)

  const choose = (next: ThemeChoice) => {
    setChoice(next)
    saveTheme(next)
    applyTheme(next, document.documentElement)
  }

  return (
    <fieldset className="theme-control">
      <legend className="sidebar-heading">Theme</legend>
      <div className="segments">
        {THEME_CHOICES.map((option) => (
          <label key={option}>
            <input
              type="radio"
              name="theme"
              value={option}
              checked={choice === option}
              onChange={() => {
                choose(option)
              }}
            />
            {LABELS[option]}
          </label>
        ))}
      </div>
    </fieldset>
  )
}
