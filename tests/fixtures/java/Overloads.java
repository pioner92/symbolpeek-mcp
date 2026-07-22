package fixtures;

// Overloading makes several declarations share one qualified name, which is
// ordinary Java rather than an edge case.
public class Overloads {
    void render() {}

    void render(int width) {}

    void render(int width, int height) {}

    static class Inner {
        void render() {}
    }

    int total() {
        return 0;
    }
}
