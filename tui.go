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
	addedStyle   = lipgloss.NewStyle().Foreground(lipgloss.Color("#00FF00")) // Green
	removedStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("#FF0000")) // Red
	headerStyle  = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("#00FFFF")) // Cyan
	contextStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("#808080")) // Gray
	cursorStyle  = lipgloss.NewStyle().Foreground(lipgloss.Color("#FF00FF")).Bold(true) // Magenta

	sidebarStyle = lipgloss.NewStyle().
			Border(lipgloss.NormalBorder(), false, true, false, false).
			BorderForeground(lipgloss.Color("#444444")).
			PaddingRight(2).
			MarginRight(2)
)

type model struct {
	files      []FileDiff
	cursorFile int
	ready      bool
	viewport   viewport.Model
	width      int
	height     int
}

func initialModel(files []FileDiff) model {
	return model{
		files:      files,
		cursorFile: 0,
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
	
	s.WriteString(headerStyle.Render(fmt.Sprintf("--- a/%s\n+++ b/%s\n\n", currFile.OldPath, currFile.NewPath)))

	for _, hunk := range currFile.Hunks {
		s.WriteString(headerStyle.Render(hunk.Header) + "\n")
		for _, line := range hunk.Lines {
			content := line.Content
			switch line.Kind {
			case "add":
				s.WriteString(addedStyle.Render("+" + content) + "\n")
			case "remove":
				s.WriteString(removedStyle.Render("-" + content) + "\n")
			default:
				s.WriteString(contextStyle.Render(" " + content) + "\n")
			}
		}
		s.WriteString("\n")
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

		headerHeight := lipgloss.Height(headerStyle.Render("Better Review - Agentic Code Review\n\n"))
		footerHeight := lipgloss.Height("\nPress up/down to navigate files, j/k to scroll diff, q to quit.")
		
		verticalMarginHeight := headerHeight + footerHeight

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
		case "up":
			if m.cursorFile > 0 {
				m.cursorFile--
				m.viewport.SetContent(m.renderDiff())
				m.viewport.GotoTop()
			}
		case "down":
			if m.cursorFile < len(m.files)-1 {
				m.cursorFile++
				m.viewport.SetContent(m.renderDiff())
				m.viewport.GotoTop()
			}
		}
	}

	// Route other messages (like j/k/pgup/pgdown) to viewport
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

	header := headerStyle.Render("Better Review - Agentic Code Review\n")

	// Render Sidebar
	var sidebar strings.Builder
	for i, f := range m.files {
		cursor := "  "
		style := lipgloss.NewStyle().Width(30).MaxWidth(30)
		if m.cursorFile == i {
			cursor = "> "
			style = style.Inherit(cursorStyle)
		}
		
		// Truncate path if too long
		displayPath := f.NewPath
		if len(displayPath) > 28 {
			displayPath = "..." + displayPath[len(displayPath)-25:]
		}
		
		sidebar.WriteString(style.Render(fmt.Sprintf("%s%s", cursor, displayPath)) + "\n")
	}

	sidebarStr := sidebarStyle.Render(sidebar.String())
	diffView := m.viewport.View()

	// Join them side-by-side
	mainContent := lipgloss.JoinHorizontal(lipgloss.Top, sidebarStr, diffView)

	footer := lipgloss.NewStyle().Foreground(lipgloss.Color("#808080")).Render("\nPress ↑/↓ to navigate files, j/k to scroll diff, q to quit.")

	return fmt.Sprintf("%s\n%s\n%s", header, mainContent, footer)
}
