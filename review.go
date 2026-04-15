package main

import (
	"fmt"
	"os/exec"
	"strings"
)

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
	err := exec.Command("git", "add", path).Run()
	if err == nil {
		f.ReviewStatus = StatusAccepted
	}
	return err
}

func RejectFile(f *FileDiff) error {
	path := f.NewPath
	if path == "" {
		path = f.OldPath
	}
	// Restore the file to discard changes in working directory
	err := exec.Command("git", "restore", path).Run()
	if err == nil {
		f.ReviewStatus = StatusRejected
	}
	return err
}

func UnstageFile(f *FileDiff) error {
	path := f.NewPath
	if path == "" {
		path = f.OldPath
	}
	err := exec.Command("git", "restore", "--staged", path).Run()
	if err == nil {
		f.ReviewStatus = StatusUnreviewed
	}
	return err
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
	cmd := exec.Command("git", "apply", "--cached", "-")
	cmd.Stdin = strings.NewReader(patch)
	err := cmd.Run()
	if err == nil {
		h.ReviewStatus = StatusAccepted
	}
	return err
}

func RejectHunk(f *FileDiff, h *Hunk) error {
	patch := PatchFromHunk(f, h)
	cmd := exec.Command("git", "apply", "--reverse", "-")
	cmd.Stdin = strings.NewReader(patch)
	err := cmd.Run()
	if err == nil {
		h.ReviewStatus = StatusRejected
	}
	return err
}
