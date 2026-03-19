package gh

import (
	"os/exec"
)

// CheckAuth runs `gh auth status` and returns true if the user is logged in.
// Exit code 0 means logged in. Exit code 4 (or any error) means not logged in.
func CheckAuth() bool {
	cmd := exec.Command("gh", "auth", "status")

	// We don't need stdout/stderr — just the exit code
	err := cmd.Run()
	return err == nil
}
