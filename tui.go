package main

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"
)

func init() {
	lipgloss.SetColorProfile(termenv.ANSI256)
}

var (
	addedStyle         = lipgloss.NewStyle().Foreground(lipgloss.Color("#3fb950"))                                                                      // GitHub/Vercel Green
	addedPrefixStyle   = lipgloss.NewStyle().Foreground(lipgloss.Color("#2ea043"))                                                                      // Dimmer Green for '+'
	removedStyle       = lipgloss.NewStyle().Foreground(lipgloss.Color("#f85149"))                                                                      // GitHub/Vercel Red
	removedPrefixStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("#da3633"))                                                                      // Dimmer Red for '-'
	headerStyle        = lipgloss.NewStyle().Foreground(lipgloss.Color("#58a6ff")).PaddingLeft(2).PaddingRight(2).Background(lipgloss.Color("#161b22")) // Block for hunks
	contextStyle       = lipgloss.NewStyle().Foreground(lipgloss.Color("#8b949e"))                                                                      // Dimmer Gray
	contextPrefixStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("#484f58"))                                                                      // Very dim gray for ' '

	activeItemStyle = lipgloss.NewStyle().
			Background(lipgloss.Color("#21262d")). // GitHub Dark Active Row
			Foreground(lipgloss.Color("#c9d1d9")). // GitHub Dark Text
			Bold(true).
			Width(30).
			MaxWidth(30).
			PaddingLeft(1)

	inactiveItemStyle = lipgloss.NewStyle().
				Foreground(lipgloss.Color("#8b949e")). // Dimmer text for inactive
				Width(30).
				MaxWidth(30).
				PaddingLeft(1)

	sidebarStyleActive = lipgloss.NewStyle().
				Border(lipgloss.NormalBorder(), false, true, false, false).
				BorderForeground(lipgloss.Color("#58a6ff")). // Subtle Blue
				PaddingRight(1).
				MarginRight(1)

	sidebarStyleInactive = lipgloss.NewStyle().
				Border(lipgloss.NormalBorder(), false, true, false, false).
				BorderForeground(lipgloss.Color("#30363d")). // Dark Gray
				PaddingRight(1).
				MarginRight(1)
)

type focusState int

const (
	focusSidebar focusState = iota
	focusViewport
)

type model struct {
	files      []FileDiff
	cursorFile int
	ready      bool
	viewport   viewport.Model
	width      int
	height     int
	focus      focusState
}

func initialModel(files []FileDiff) model {
	return model{
		files:      files,
		cursorFile: 0,
		focus:      focusSidebar,
	}
}

func (m model) Init() tea.Cmd {
	return nil
}

func (m *model) renderDiff() string {
	if len(m.files) == 0 {
		return "No changes."
	}
	var s strings.Builder
	currFile := m.files[m.cursorFile]

	for _, hunk := range currFile.Hunks {
		// Render hunk header as a subtle block
		s.WriteString("\n" + headerStyle.Render(hunk.Header) + "\n")
		for _, line := range hunk.Lines {
			content := line.Content
			switch line.Kind {
			case "add":
				s.WriteString(addedPrefixStyle.Render("+ ") + addedStyle.Render(content) + "\n")
			case "remove":
				s.WriteString(removedPrefixStyle.Render("- ") + removedStyle.Render(content) + "\n")
			default:
				s.WriteString(contextPrefixStyle.Render("  ") + contextStyle.Render(content) + "\n")
			}
		}
	}
	return s.String()
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var (
		cmd  tea.Cmd
		cmds []tea.Cmd
	)

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height

		footerHeight := lipgloss.Height("\nPress ↑/↓ to navigate files, Enter to view diff, Esc to return, q to quit.")
		verticalMarginHeight := footerHeight + 1 // Add 1 for the gap

		if !m.ready {
			m.viewport = viewport.New(m.width-35, m.height-verticalMarginHeight) // Assumes 35 chars for sidebar
			m.viewport.SetContent(m.renderDiff())
			m.ready = true
		} else {
			m.viewport.Width = m.width - 35
			m.viewport.Height = m.height - verticalMarginHeight
		}

	case tea.KeyMsg:
		switch msg.String() {
		case "ctrl+c", "q":
			return m, tea.Quit

		case "enter":
			if m.focus == focusSidebar {
				m.focus = focusViewport
			}

		case "esc":
			if m.focus == focusViewport {
				m.focus = focusSidebar
			}

		case "up", "k":
			if m.focus == focusSidebar {
				if m.cursorFile > 0 {
					m.cursorFile--
					m.viewport.SetContent(m.renderDiff())
					m.viewport.GotoTop()
				}
			} else if m.focus == focusViewport {
				m.viewport.LineUp(1)
			}

		case "down", "j":
			if m.focus == focusSidebar {
				if m.cursorFile < len(m.files)-1 {
					m.cursorFile++
					m.viewport.SetContent(m.renderDiff())
					m.viewport.GotoTop()
				}
			} else if m.focus == focusViewport {
				m.viewport.LineDown(1)
			}
		}
	}

	// Always route commands to viewport to handle things like mouse wheel
	m.viewport, cmd = m.viewport.Update(msg)
	cmds = append(cmds, cmd)

	return m, tea.Batch(cmds...)
}

func (m model) View() string {
	if !m.ready {
		return "\n  Initializing..."
	}

	if len(m.files) == 0 {
		return "No changes found.\nPress q to quit."
	}

	// Render Sidebar
	var sidebar strings.Builder

	// Add a subtle title to the sidebar instead of a global title
	sidebar.WriteString(lipgloss.NewStyle().Foreground(lipgloss.Color("#8b949e")).Bold(true).PaddingLeft(1).Render("FILES") + "\n\n")

	for i, f := range m.files {
		style := inactiveItemStyle

		if m.cursorFile == i {
			if m.focus == focusSidebar {
				style = activeItemStyle
			} else {
				// Focused on viewport, keep background but dim text
				style = activeItemStyle.Copy().Foreground(lipgloss.Color("#8b949e"))
			}
		}

		// Truncate path if too long
		displayPath := f.NewPath
		if len(displayPath) > 28 {
			displayPath = "..." + displayPath[len(displayPath)-25:]
		}

		sidebar.WriteString(style.Render(displayPath) + "\n")
	}

	var sidebarStr string
	if m.focus == focusSidebar {
		sidebarStr = sidebarStyleActive.Render(sidebar.String())
	} else {
		sidebarStr = sidebarStyleInactive.Render(sidebar.String())
	}

	diffView := m.viewport.View()

	// Join them side-by-side
	mainContent := lipgloss.JoinHorizontal(lipgloss.Top, sidebarStr, diffView)

	statusBg := lipgloss.Color("#21262d")
	statusFg := lipgloss.Color("#c9d1d9")
	if m.focus == focusViewport {
		statusBg = lipgloss.Color("#58a6ff")
		statusFg = lipgloss.Color("#0d1117")
	}

	modeStr := " FILES "
	if m.focus == focusViewport {
		modeStr = " REVIEWING "
	}

	pill := lipgloss.NewStyle().
		Background(statusBg).
		Foreground(statusFg).
		Bold(true).
		Render(modeStr)

	footerText := "↑/↓: select file | Enter: review | q: quit"
	if m.focus == focusViewport {
		footerText = "↑/↓: scroll code | Esc: back to files | q: quit"
	}

	helpText := lipgloss.NewStyle().Foreground(lipgloss.Color("#8b949e")).Render(" " + footerText)
	footer := lipgloss.JoinHorizontal(lipgloss.Top, pill, helpText)

	return fmt.Sprintf("\n%s\n\n%s", mainContent, footer)
}
