package main 
import (
	"os/exec"
)
//pass args as parameter to the function
func runCommand(command string,args ...string) (string,error){
	cmd := exec.Command(command, args...)
	output, err := cmd.CombinedOutput()

	return string(output), err
}

func main() {
	output,err := runCommand("git", "diff")
	if err != nil {
		println("Error:", err.Error())
	} else {
		println("Output:", output)
	}
}
