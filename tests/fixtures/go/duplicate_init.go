package fixtures

import "fmt"

// Go allows any number of init functions per file, so a file can legitimately
// hold several declarations sharing one name.

var registry = map[string]int{}

func init() {
	registry["first"] = 1
}

func init() {
	registry["second"] = 2
}

type Store struct{ count int }

func (s *Store) Get() int { return s.count }

func Report() string { return fmt.Sprint(registry) }
