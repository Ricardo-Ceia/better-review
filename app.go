package main

import (
	"context"
	"errors"
	"fmt"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"
)

var debugLog *os.File

func init() {
	lipgloss.SetColorProfile(termenv.ANSI256)
}

var (
	baseBackground  = lipgloss.Color("#101418")
	panelBackground = lipgloss.Color("#171f24")
	panelMuted      = lipgloss.Color("#22303a")
	textPrimary     = lipgloss.Color("#edf2f7")
	textMuted       = lipgloss.Color("#93a6b5")
	textSubtle      = lipgloss.Color("#607282")
	accentColor     = lipgloss.Color("#f4b942")
	accentDeep      = lipgloss.Color("#c98d1d")
	dangerColor     = lipgloss.Color("#ef6f6c")
	successColor    = lipgloss.Color("#59c9a5")

	shellStyle = lipgloss.NewStyle().
			Background(baseBackground).
			Foreground(textPrimary)

	cardStyle = lipgloss.NewStyle().
			Background(panelBackground).
			Border(lipgloss.RoundedBorder()).
			BorderForeground(panelMuted).
			Padding(1, 2)

	heroStyle = lipgloss.NewStyle().
			Foreground(textPrimary).
			Bold(true)

	subtleStyle = lipgloss.NewStyle().
			Foreground(textMuted)

	statusIdleStyle = lipgloss.NewStyle().
			Foreground(textPrimary).
			Background(panelMuted).
			Bold(true).
			Padding(0, 1)

	statusBusyStyle = lipgloss.NewStyle().
			Foreground(baseBackground).
			Background(accentColor).
			Bold(true).
			Padding(0, 1)

	statusErrorStyle = lipgloss.NewStyle().
				Foreground(textPrimary).
				Background(dangerColor).
				Bold(true).
				Padding(0, 1)

	sectionTitleStyle = lipgloss.NewStyle().
				Foreground(accentColor).
				Bold(true)

	historyTimeStyle = lipgloss.NewStyle().
				Foreground(textSubtle)

	historyPromptStyle = lipgloss.NewStyle().
				Foreground(textPrimary).
				Bold(true)

	historyMetaStyle = lipgloss.NewStyle().
				Foreground(textMuted)

	hintStyle = lipgloss.NewStyle().
			Foreground(textSubtle)

	shortcutStyle = lipgloss.NewStyle().
			Foreground(accentColor).
			Bold(true)

	inputStyle = lipgloss.NewStyle().
			Foreground(textPrimary)

	inputPromptStyle = lipgloss.NewStyle().
				Foreground(accentColor).
				Bold(true)

	inputBorderStyle = lipgloss.NewStyle().
				Border(lipgloss.RoundedBorder()).
				BorderForeground(accentDeep).
				Background(panelBackground).
				Padding(0, 1)
)

type appMode int

const (
	modePrompt appMode = iota
	modeReview
)

type runStatus int

const (
	statusIdle runStatus = iota
	statusRunning
	statusFailed
)

type promptRun struct {
	Prompt         string
	StartedAt      time.Time
	FinishedAt     time.Time
	ChangedFiles   int
	HasDiff        bool
	FailureMessage string
	Command        string
}

type opencodeRunResult struct {
	Run    promptRun
	Files  []FileDiff
	Err    error
	Stdout string
	Stderr string
}

type runFinishedMsg struct {
	result opencodeRunResult
}

type appModel struct {
	repoPath      string
	runner        *OpencodeRunner
	mode          appMode
	runStatus     runStatus
	ready         bool
	width         int
	height        int
	statusMessage string
	lastRun       *promptRun
	history       []promptRun
	promptInput   textinput.Model
	historyView   viewport.Model
	review        reviewModel
	runCounter    int
}

func newAppModel(repoPath string, runner *OpencodeRunner) appModel {
	input := textinput.New()
	input.Prompt = ""
	input.Placeholder = "Describe the change you want opencode to make"
	input.Focus()
	input.CharLimit = 0
	input.Width = 80
	input.TextStyle = inputStyle
	input.PromptStyle = inputPromptStyle

	return appModel{
		repoPath:      repoPath,
		runner:        runner,
		mode:          modePrompt,
		runStatus:     statusIdle,
		statusMessage: "Ready for your next change request.",
		promptInput:   input,
		history:       []promptRun{},
		review:        newReviewModel(nil),
	}
}

func (m appModel) Init() tea.Cmd {
	return textinput.Blink
}

func (m appModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		m.resize()

	case runFinishedMsg:
		m.runStatus = statusIdle
		if msg.result.Err != nil {
			m.runStatus = statusFailed
			failedRun := msg.result.Run
			failedRun.FailureMessage = msg.result.Err.Error()
			m.lastRun = &failedRun
			m.history = append([]promptRun{failedRun}, m.history...)
			m.statusMessage = failedRun.FailureMessage
			m.refreshHistory()
			return m, nil
		}

		finishedRun := msg.result.Run
		m.lastRun = &finishedRun
		m.history = append([]promptRun{finishedRun}, m.history...)
		if finishedRun.HasDiff {
			m.mode = modeReview
			m.review = newReviewModel(msg.result.Files)
			m.review.resize(m.width, m.height)
			m.statusMessage = fmt.Sprintf("Run %d finished. Review %d changed file(s).", m.runCounter, finishedRun.ChangedFiles)
		} else {
			m.mode = modePrompt
			m.review = newReviewModel(nil)
			m.statusMessage = "Run finished with no code changes."
		}
		m.refreshHistory()

	case tea.KeyMsg:
		if m.mode == modeReview {
			switch msg.String() {
			case "ctrl+c":
				return m, tea.Quit
			case "esc":
				if !m.review.inDiffView() {
					m.mode = modePrompt
					m.statusMessage = "Returned to prompt."
					m.promptInput.Focus()
					return m, nil
				}
			}

			updatedReview, cmd := m.review.Update(msg)
			m.review = updatedReview
			return m, cmd
		}

		switch msg.String() {
		case "ctrl+c":
			return m, tea.Quit
		case "ctrl+o":
			if m.review.hasFiles() {
				m.mode = modeReview
				m.statusMessage = "Reviewing current code changes."
				return m, nil
			}
		case "enter":
			if m.runStatus == statusRunning {
				return m, nil
			}
			prompt := strings.TrimSpace(m.promptInput.Value())
			if prompt == "" {
				m.statusMessage = "Write a prompt first."
				return m, nil
			}

			m.runCounter++
			m.runStatus = statusRunning
			m.statusMessage = fmt.Sprintf("Running opencode on prompt %d...", m.runCounter)
			m.promptInput.SetValue("")
			m.refreshHistory()
			return m, m.runPromptCmd(prompt)
		}
	}

	if m.mode == modePrompt {
		var cmd tea.Cmd
		m.promptInput, cmd = m.promptInput.Update(msg)
		cmds = append(cmds, cmd)
	}

	return m, tea.Batch(cmds...)
}

func (m appModel) View() string {
	if !m.ready {
		return shellStyle.Render("\n  Loading better-review...")
	}

	if m.mode == modeReview {
		return shellStyle.Render(m.review.View())
	}

	return shellStyle.Render(m.renderPromptView())
}

func (m *appModel) resize() {
	if m.width <= 0 || m.height <= 0 {
		return
	}

	inputWidth := m.width - 10
	if inputWidth < 20 {
		inputWidth = 20
	}
	m.promptInput.Width = inputWidth

	historyWidth := m.width - 8
	if historyWidth < 20 {
		historyWidth = 20
	}

	headerHeight := lipgloss.Height(m.renderHeader())
	footerHeight := lipgloss.Height(m.renderFooter())
	inputHeight := lipgloss.Height(m.renderInputCard())
	historyHeight := m.height - headerHeight - footerHeight - inputHeight - 8
	if historyHeight < 6 {
		historyHeight = 6
	}

	if m.historyView.Width == 0 {
		m.historyView = viewport.New(historyWidth, historyHeight)
	} else {
		m.historyView.Width = historyWidth
		m.historyView.Height = historyHeight
	}

	m.review.resize(m.width, m.height)
	m.refreshHistory()
}

func (m *appModel) renderPromptView() string {
	header := m.renderHeader()
	history := cardStyle.Width(m.width - 4).Render(m.renderHistoryCard())
	input := cardStyle.Width(m.width - 4).Render(m.renderInputCard())
	footer := m.renderFooter()

	return lipgloss.JoinVertical(lipgloss.Left, header, "", history, "", input, "", footer)
}

func (m *appModel) renderHeader() string {
	status := statusIdleStyle.Render("IDLE")
	switch m.runStatus {
	case statusRunning:
		status = statusBusyStyle.Render("RUNNING")
	case statusFailed:
		status = statusErrorStyle.Render("FAILED")
	}

	projectName := filepath.Base(m.repoPath)
	title := heroStyle.Render("better-review") + "  " + subtleStyle.Render(projectName)
	meta := subtleStyle.Render(m.statusMessage)

	return cardStyle.Width(m.width - 4).Render(lipgloss.JoinVertical(lipgloss.Left,
		lipgloss.JoinHorizontal(lipgloss.Center, title, "   ", status),
		"",
		meta,
	))
}

func (m *appModel) renderHistoryCard() string {
	title := sectionTitleStyle.Render("Recent runs")
	if len(m.history) == 0 {
		return lipgloss.JoinVertical(lipgloss.Left,
			title,
			"",
			subtleStyle.Render("No runs yet. Submit a prompt to generate code changes."),
		)
	}

	return lipgloss.JoinVertical(lipgloss.Left, title, "", m.historyView.View())
}

func (m *appModel) renderInputCard() string {
	title := sectionTitleStyle.Render("Prompt")
	hints := lipgloss.JoinHorizontal(lipgloss.Left,
		hintStyle.Render("Press "), shortcutStyle.Render("Enter"), hintStyle.Render(" to run in the background."),
	)
	if m.review.hasFiles() {
		hints = lipgloss.JoinHorizontal(lipgloss.Left,
			hints,
			hintStyle.Render("  Press "), shortcutStyle.Render("Ctrl+O"), hintStyle.Render(" to reopen the current review."),
		)
	}

	inputBox := inputBorderStyle.Width(m.width - 12).Render(inputPromptStyle.Render("> ") + m.promptInput.View())
	return lipgloss.JoinVertical(lipgloss.Left, title, "", hints, "", inputBox)
}

func (m *appModel) renderFooter() string {
	left := subtleStyle.Render("Only code changes are shown. opencode output stays hidden.")
	right := subtleStyle.Render("Ctrl+C quits")
	if m.review.hasFiles() {
		right = subtleStyle.Render("Ctrl+O review  |  Ctrl+C quit")
	}
	return lipgloss.NewStyle().Width(m.width - 4).Render(lipgloss.JoinHorizontal(lipgloss.Left, left, strings.Repeat(" ", max(1, m.width-lipgloss.Width(left)-lipgloss.Width(right)-8)), right))
}

func (m *appModel) refreshHistory() {
	if m.historyView.Width == 0 {
		return
	}

	var sections []string
	for _, run := range m.history {
		sections = append(sections, renderRunSummary(run, m.historyView.Width))
	}
	m.historyView.SetContent(strings.Join(sections, "\n\n"))
	m.historyView.GotoTop()
}

func renderRunSummary(run promptRun, width int) string {
	timestamp := historyTimeStyle.Render(run.FinishedAt.Format("15:04:05"))
	prompt := historyPromptStyle.Render(strings.TrimSpace(run.Prompt))
	metaText := fmt.Sprintf("command: %s", run.Command)
	if run.FailureMessage != "" {
		metaText = fmt.Sprintf("failed: %s", run.FailureMessage)
	} else if run.HasDiff {
		metaText = fmt.Sprintf("%d file(s) changed", run.ChangedFiles)
	} else {
		metaText = "no code changes"
	}
	meta := historyMetaStyle.Render(metaText)
	return lipgloss.NewStyle().Width(width).Render(lipgloss.JoinVertical(lipgloss.Left, timestamp, prompt, meta))
}

func (m *appModel) runPromptCmd(prompt string) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
		defer cancel()

		result := m.runner.RunPrompt(ctx, prompt, m.runCounter)
		return runFinishedMsg{result: result}
	}
}

type OpencodeRunner struct {
	repoPath string
	binary   string
}

func NewOpencodeRunner(repoPath, requestedBinary string) *OpencodeRunner {
	binary := strings.TrimSpace(requestedBinary)
	if binary == "" {
		binary = defaultOpencodeBinary(repoPath)
	}
	return &OpencodeRunner{repoPath: repoPath, binary: binary}
}

func defaultOpencodeBinary(repoPath string) string {
	return "opencode"
}

func (r *OpencodeRunner) RunPrompt(ctx context.Context, prompt string, runNumber int) opencodeRunResult {
	startedAt := time.Now()
	result := opencodeRunResult{
		Run: promptRun{
			Prompt:    prompt,
			StartedAt: startedAt,
			Command:   r.commandLabel(),
		},
	}

	beforeDiff, err := CollectGitDiff(ctx, r.repoPath)
	if err != nil {
		result.Run.FinishedAt = time.Now()
		result.Err = err
		return result
	}

	stdout, stderr, err := r.execute(ctx, prompt)
	result.Stdout = stdout
	result.Stderr = stderr
	if stdout != "" || stderr != "" {
		log.Printf("run %d completed command %q (stdout=%d bytes, stderr=%d bytes)", runNumber, r.commandLabel(), len(stdout), len(stderr))
	}
	if err != nil {
		result.Run.FinishedAt = time.Now()
		result.Err = fmt.Errorf("opencode run failed: %w", err)
		return result
	}

	afterDiff, err := CollectGitDiff(ctx, r.repoPath)
	if err != nil {
		result.Run.FinishedAt = time.Now()
		result.Err = err
		return result
	}

	files, err := ParseGitDiff(afterDiff)
	if err != nil {
		result.Run.FinishedAt = time.Now()
		result.Err = fmt.Errorf("parse git diff: %w", err)
		return result
	}

	result.Run.FinishedAt = time.Now()
	result.Run.HasDiff = strings.TrimSpace(afterDiff) != "" && len(files) > 0
	result.Run.ChangedFiles = len(files)
	result.Files = files
	if beforeDiff == afterDiff && result.Run.HasDiff {
		log.Printf("run %d finished without changing the existing diff", runNumber)
	}
	return result
}

func (r *OpencodeRunner) execute(ctx context.Context, prompt string) (string, string, error) {
	args := []string{"run", "--dir", r.repoPath, "--format", "json", prompt}
	cmd := exec.CommandContext(ctx, r.binary, args...)
	cmd.Dir = r.repoPath

	var stdoutBuf strings.Builder
	var stderrBuf strings.Builder
	cmd.Stdout = &stdoutBuf
	cmd.Stderr = &stderrBuf
	err := cmd.Run()
	if err == nil {
		return stdoutBuf.String(), stderrBuf.String(), nil
	}

	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) {
		return stdoutBuf.String(), stderrBuf.String(), err
	}
	if stdoutBuf.Len() == 0 && stderrBuf.Len() == 0 {
		stderrBuf.WriteString(err.Error())
	}
	return stdoutBuf.String(), stderrBuf.String(), err
}

func (r *OpencodeRunner) commandLabel() string {
	base := filepath.Base(r.binary)
	if base == "." || base == string(filepath.Separator) || base == "" {
		return r.binary
	}
	return base + " run"
}

func CollectGitDiff(ctx context.Context, repoPath string) (string, error) {
	trackedDiff, err := runCommandOutput(ctx, repoPath, "git", "diff", "--no-color", "--no-ext-diff")
	if err != nil {
		return "", fmt.Errorf("failed to run git diff: %w", err)
	}

	untrackedDiff, err := collectUntrackedDiff(ctx, repoPath)
	if err != nil {
		return "", err
	}

	if trackedDiff == "" {
		return untrackedDiff, nil
	}
	if untrackedDiff == "" {
		return trackedDiff, nil
	}
	return trackedDiff + "\n" + untrackedDiff, nil
}

func collectUntrackedDiff(ctx context.Context, repoPath string) (string, error) {
	paths, err := listUntrackedFiles(ctx, repoPath)
	if err != nil {
		return "", err
	}

	var diffs []string
	for _, path := range paths {
		diff, err := diffForUntrackedFile(ctx, repoPath, path)
		if err != nil {
			return "", err
		}
		if strings.TrimSpace(diff) != "" {
			diffs = append(diffs, diff)
		}
	}

	return strings.Join(diffs, "\n"), nil
}

func listUntrackedFiles(ctx context.Context, repoPath string) ([]string, error) {
	out, err := runCommandOutput(ctx, repoPath, "git", "ls-files", "--others", "--exclude-standard", "-z")
	if err != nil {
		return nil, fmt.Errorf("list untracked files: %w", err)
	}

	if out == "" {
		return nil, nil
	}

	parts := strings.Split(out, "\x00")
	paths := make([]string, 0, len(parts))
	for _, part := range parts {
		if part == "" {
			continue
		}
		paths = append(paths, part)
	}
	return paths, nil
}

func diffForUntrackedFile(ctx context.Context, repoPath, path string) (string, error) {
	cmd := exec.CommandContext(ctx, "git", "diff", "--no-index", "--no-color", "--", "/dev/null", path)
	cmd.Dir = repoPath

	out, err := cmd.Output()
	if err == nil {
		return string(out), nil
	}

	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) && exitErr.ExitCode() == 1 {
		return string(out), nil
	}

	return "", fmt.Errorf("diff untracked file %s: %w", path, err)
}

func runCommandOutput(ctx context.Context, dir, name string, args ...string) (string, error) {
	cmd := exec.CommandContext(ctx, name, args...)
	cmd.Dir = dir
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return string(out), nil
}

func initLogger() error {
	file, err := os.OpenFile("debug.log", os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0666)
	if err != nil {
		return err
	}
	debugLog = file
	log.SetOutput(file)
	log.SetFlags(log.Ltime | log.Lmicroseconds | log.Lshortfile)
	log.Println("Logger initialized")
	return nil
}

func closeLogger() {
	if debugLog != nil {
		_ = debugLog.Close()
		debugLog = nil
	}
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}
