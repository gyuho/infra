package prompt

import "fmt"

func IsInputYes(msg string) bool {
	fmt.Println(msg)
	fmt.Printf("Type 'yes' to continue, otherwise skip this operation: ")
	var confirm string
	fmt.Scanln(&confirm)
	return confirm == "yes" || confirm == "y"
}
