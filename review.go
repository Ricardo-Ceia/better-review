package main

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
)

func gitCommand(args ...string) *exec.Cmd {
	cmd := exec.Command("git", args...)
	if cwd, err := os.Getwd(); err == nil {
		cmd.Dir = cwd
	}
	return cmd
}

type ReviewStatus string

const (
	StatusUnreviewed ReviewStatus = ""
	StatusAccepted   ReviewStatus = "accepted"
	StatusRejected   ReviewStatus = "rejected"
)

func AcceptFile(f *FileDiff) error {
	path := f.NewPath
	if path == "" {
		path = f.OldPath
	}
	err := gitCommand("add", "--", path).Run()
	if err == nil {
		f.ReviewStatus = StatusAccepted
		setAllHunksStatus(f, StatusAccepted)
	}
	return err
}

func RejectFile(f *FileDiff) error {
	path := f.NewPath
	if path == "" {
		path = f.OldPath
	}

	var err error
	if f.Status == "added" {
		err = rejectAddedFile(path)
	} else {
		err = gitCommand("restore", "--source=HEAD", "--staged", "--worktree", "--", path).Run()
	}
	if err == nil {
		f.ReviewStatus = StatusRejected
		setAllHunksStatus(f, StatusRejected)
	}
	return err
}

func UnstageFile(f *FileDiff) error {
	path := f.NewPath
	if path == "" {
		path = f.OldPath
	}
	err := gitCommand("restore", "--staged", "--", path).Run()
	if err == nil {
		f.ReviewStatus = StatusUnreviewed
		setAllHunksStatus(f, StatusUnreviewed)
	}
	return err
}

func rejectAddedFile(path string) error {
	if gitCommand("ls-files", "--error-unmatch", "--", path).Run() == nil {
		return gitCommand("rm", "-f", "--", path).Run()
	}

	cwd, err := os.Getwd()
	if err != nil {
		return err
	}

	return os.RemoveAll(filepath.Join(cwd, path))
}

func setAllHunksStatus(file *FileDiff, status ReviewStatus) {
	for i := range file.Hunks {
		file.Hunks[i].ReviewStatus = status
	}
}

func PatchFromHunk(file *FileDiff, hunk *Hunk) string {
	var sb strings.Builder

	oldPath := "a/" + file.OldPath
	if file.OldPath == "" {
		oldPath = "/dev/null"
	}

	newPath := "b/" + file.NewPath
	if file.NewPath == "" {
		newPath = "/dev/null"
	}

	sb.WriteString(fmt.Sprintf("--- %s\n", oldPath))
	sb.WriteString(fmt.Sprintf("+++ %s\n", newPath))
	sb.WriteString(hunk.Header + "\n")
	for _, line := range hunk.Lines {
		prefix := " "
		if line.Kind == "add" {
			prefix = "+"
		} else if line.Kind == "remove" {
			prefix = "-"
		}
		sb.WriteString(prefix + line.Content + "\n")
	}
	return sb.String()
}

func AcceptHunk(f *FileDiff, h *Hunk) error {
	patch := PatchFromHunk(f, h)
	cmd := gitCommand("apply", "--cached", "-")
	cmd.Stdin = strings.NewReader(patch)
	err := cmd.Run()
	if err == nil {
		h.ReviewStatus = StatusAccepted
		syncFileReviewStatus(f)
	}
	return err
}

func RejectHunk(f *FileDiff, h *Hunk) error {
	patch := PatchFromHunk(f, h)
	cmd := gitCommand("apply", "--reverse", "-")
	cmd.Stdin = strings.NewReader(patch)
	err := cmd.Run()
	if err == nil {
		h.ReviewStatus = StatusRejected
		syncFileReviewStatus(f)
	}
	return err
}

func syncFileReviewStatus(file *FileDiff) {
	if len(file.Hunks) == 0 {
		return
	}

	accepted := true
	rejected := true
	for _, hunk := range file.Hunks {
		accepted = accepted && hunk.ReviewStatus == StatusAccepted
		rejected = rejected && hunk.ReviewStatus == StatusRejected
	}

	switch {
	case accepted:
		file.ReviewStatus = StatusAccepted
	case rejected:
		file.ReviewStatus = StatusRejected
	default:
		file.ReviewStatus = StatusUnreviewed
	}
}
