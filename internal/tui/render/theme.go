package render

import (
	"image/color"
	"os"

	"charm.land/lipgloss/v2"
)

// Theme holds all colors used across the application.
// Components read from this struct instead of hardcoding colors.
type Theme struct {
	// Backgrounds
	HeaderBg color.Color
	TabBarBg color.Color
	PaneBg   color.Color

	// Borders
	BorderActive   color.Color
	BorderInactive color.Color

	// Text
	TextPrimary   color.Color
	TextSecondary color.Color
	TextMuted     color.Color

	// Status colors (shared across themes)
	StatusGreen  color.Color
	StatusRed    color.Color
	StatusYellow color.Color
	StatusBlue   color.Color

	// Cursor / selection
	CursorBg color.Color
	CursorFg color.Color

	// Diff colors
	DiffAddBg    color.Color
	DiffRemoveBg color.Color
	DiffAddFg    color.Color
	DiffRemoveFg color.Color
}

// DarkTheme returns the default dark color palette (GitHub Dark-inspired).
func DarkTheme() Theme {
	return Theme{
		HeaderBg:       lipgloss.Color("#1E1E1E"),
		TabBarBg:       lipgloss.Color("#161B22"),
		PaneBg:         lipgloss.Color("#0D1117"),
		BorderActive:   lipgloss.Color("#58A6FF"),
		BorderInactive: lipgloss.Color("#484F58"),
		TextPrimary:    lipgloss.Color("#FFFFFF"),
		TextSecondary:  lipgloss.Color("#8B949E"),
		TextMuted:      lipgloss.Color("#484F58"),
		StatusGreen:    lipgloss.Color("#3FB950"),
		StatusRed:      lipgloss.Color("#F85149"),
		StatusYellow:   lipgloss.Color("#D29922"),
		StatusBlue:     lipgloss.Color("#58A6FF"),
		CursorBg:       lipgloss.Color("#264F78"),
		CursorFg:       lipgloss.Color("#FFFFFF"),
		DiffAddBg:      lipgloss.Color("#1B3A2A"),
		DiffRemoveBg:   lipgloss.Color("#3A1B1B"),
		DiffAddFg:      lipgloss.Color("#3FB950"),
		DiffRemoveFg:   lipgloss.Color("#F85149"),
	}
}

// LightTheme returns a light color palette (GitHub Light-inspired).
func LightTheme() Theme {
	return Theme{
		HeaderBg:       lipgloss.Color("#F6F8FA"),
		TabBarBg:       lipgloss.Color("#FFFFFF"),
		PaneBg:         lipgloss.Color("#FFFFFF"),
		BorderActive:   lipgloss.Color("#0969DA"),
		BorderInactive: lipgloss.Color("#D0D7DE"),
		TextPrimary:    lipgloss.Color("#24292F"),
		TextSecondary:  lipgloss.Color("#57606A"),
		TextMuted:      lipgloss.Color("#8C959F"),
		StatusGreen:    lipgloss.Color("#1A7F37"),
		StatusRed:      lipgloss.Color("#CF222E"),
		StatusYellow:   lipgloss.Color("#9A6700"),
		StatusBlue:     lipgloss.Color("#0969DA"),
		CursorBg:       lipgloss.Color("#DDF4FF"),
		CursorFg:       lipgloss.Color("#24292F"),
		DiffAddBg:      lipgloss.Color("#DAFBE1"),
		DiffRemoveBg:   lipgloss.Color("#FFEBE9"),
		DiffAddFg:      lipgloss.Color("#1A7F37"),
		DiffRemoveFg:   lipgloss.Color("#CF222E"),
	}
}

// CurrentTheme returns the appropriate theme based on the config value.
// "system" uses the COLORFGBG environment variable as a heuristic:
// if the background component is < 8, the terminal is dark.
func CurrentTheme(themeName string) Theme {
	switch themeName {
	case "light":
		return LightTheme()
	case "system":
		if isSystemDark() {
			return DarkTheme()
		}
		return LightTheme()
	default:
		return DarkTheme()
	}
}

// isSystemDark checks the COLORFGBG environment variable to guess the terminal
// background darkness. Format is "fg;bg" where bg < 8 typically means dark.
// Falls back to dark (true) if the variable isn't set or can't be parsed.
func isSystemDark() bool {
	val := os.Getenv("COLORFGBG")
	if val == "" {
		return true // default to dark when unknown
	}
	parts := splitLast(val, ";")
	if len(parts) != 2 {
		return true
	}
	// ANSI color 0-7 are dark, 8-15 are bright
	bg := parts[1]
	return bg == "" || bg == "0" || bg == "1" || bg == "2" || bg == "3" ||
		bg == "4" || bg == "5" || bg == "6" || bg == "7"
}

// splitLast splits a string on the last occurrence of sep.
func splitLast(s, sep string) []string {
	i := len(s) - 1
	for i >= 0 {
		if string(s[i]) == sep {
			return []string{s[:i], s[i+1:]}
		}
		i--
	}
	return []string{s}
}
