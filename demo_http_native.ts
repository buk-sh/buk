// http-native framework demo

async function main() {
    const app = createApp();

    // Basic routes
    app.get("/", async (req, res) => {
        res.json({ message: "Hello from http-native!" });
    });

    app.get("/user/:id", async (req, res) => {
        res.json({ id: req.params.id });
    });

    app.post("/users", async (req, res) => {
        res.status(201).json({ created: true });
    });

    // Route grouping
    app.group("/api/v1", (api) => {
        api.get("/users", async (req, res) => {
            res.json({ users: [] });
        });
        
        api.post("/users", async (req, res) => {
            res.status(201).json({ created: true });
        });
    });

    // Error handling
    app.error(async (error, req, res) => {
        res.status(500).json({ error: error.message });
    });

    // Start server
    const server = await app.listen().port(3000);
    console.log("Server running at " + server.url);
    console.log("Press Ctrl+C to stop");
    
    // Keep the process alive
    while (true) {
        Bun.sleep(1000);
    }
}

main();
