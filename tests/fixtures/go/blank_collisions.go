package p

// The receiver type is not declared in this file, so the method is a top-level
// node displayed as `render` — the same leaf as the const below. A reader that
// composes `render` must land on exactly one of them.
func (w *Widget) render() string { return render }

const render = "x"

// Both blanks are anchored to the one var spec, so they share a source
// position; `@line:column` alone cannot tell them apart.
var _, _ = pair()

func pair() (int, int) { return 0, 0 }

// A blank method (receiver type absent here) is a top-level `_`; its bare
// display must not stay ambiguous against the blank vars that follow.
func (Gadget) _() {}

var (
	_ = 1
	_ = 2
)
