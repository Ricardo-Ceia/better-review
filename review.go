package main

import (
	"os/exec"
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
