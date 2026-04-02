// Demo of Runt's Bun-like features

console.log("=== Runt Bun-like Features Demo ===\n");

// Bun.env - Environment variables
console.log("Environment variables:");
console.log("PATH exists:", "PATH" in Bun.env);
console.log("HOME exists:", "HOME" in Bun.env || "USERPROFILE" in Bun.env);

// Bun.cwd() - Current working directory
console.log("\nCurrent directory:", Bun.cwd());

// Bun.main - Main script path
console.log("Main script:", Bun.main);

// Bun.which - Find executable
console.log("\nFinding 'node':", Bun.which("node") || "not found");
console.log("Finding 'cargo':", Bun.which("cargo") || "not found");

// Bun.file() - File operations
const testFile = Bun.file("test.txt");
console.log("\nFile operations:");
console.log("test.txt exists:", testFile.exists());

// Bun.write() - Write file
console.log("\nWriting to test_write.txt...");
const writeSuccess = Bun.write("test_write.txt", "Hello from Bun.write!");
console.log("Write success:", writeSuccess);

// Read it back
const writtenFile = Bun.file("test_write.txt");
console.log("Read back:", writtenFile.text());

// File size
console.log("File size:", writtenFile.size(), "bytes");

// Bun.sleep - Sleep (blocking, use sparingly)
console.log("\nSleeping for 100ms...");
console.time("sleep");
Bun.sleep(100);
console.timeEnd("sleep");

console.log("\n=== Demo Complete ===");
