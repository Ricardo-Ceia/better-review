package main

import (
	"log"
	"os/exec"
	"strings"
)

type DiffInfo struct {
	NumberOfAddedLines   int
	NumberOfRemovedLines int
	RemovedLines				 []string
	AddedLines					 []string
	ModifiedFiles []string
}

func runCommand(command string,args ...string) (string,error){
	cmd := exec.Command(command, args...)
	output, err := cmd.CombinedOutput()

	return string(output), err
}

func parseOutput(output string) (DiffInfo,error){
	var diffInfo DiffInfo
	lines := strings.Split(output, "\n")
	for i, line := range lines {
		log.Println(line)
		// first line of the git diff output contains the file name
		if i==0{
			lineParts := strings.Split(line, " ")
			// the file name is in the 4th part of the line, and it starts with "b/"
			diffInfo.ModifiedFiles = append(diffInfo.ModifiedFiles, strings.TrimPrefix(lineParts[3],"b/"))
		}
		if !strings.HasPrefix(line, "+++") && strings.HasPrefix(line, "+"){
			diffInfo.NumberOfAddedLines++
			diffInfo.AddedLines = append(diffInfo.AddedLines, line[1:])
		}
		if !strings.HasPrefix(line, "---") && strings.HasPrefix(line, "-"){
			diffInfo.NumberOfRemovedLines++
			diffInfo.RemovedLines = append(diffInfo.RemovedLines, line[1:])
		}
	}
	return diffInfo, nil
}

func main() {
	output,err := runCommand("git", "diff")
	if err != nil {
		println("Error:", err.Error())
	} 

	diffInfo, err := parseOutput(output)

	if err != nil {
		println("Error:", err.Error())
	}else{
		println("Modified Files:", diffInfo.ModifiedFiles[0])
		println("Number of Added Lines:", diffInfo.NumberOfAddedLines)
		println("Number of Removed Lines:", diffInfo.NumberOfRemovedLines)
	}
}
