// Demo of Runt's Bun-like features

console.log("=== Runt Bun-like Features Demo ===\n");

// Bun.env - Environment variables
console.log("Environment variables:");
console.log("PATH exists:", "PATH" in Buk.env);
console.log("HOME exists:", "HOME" in Buk.env || "USERPROFILE" in Bun.env);

// Bun.cwd() - Current working directory
console.log("\nCurrent directory:", Buk.cwd());

// Bun.main - Main script path
console.log("Main script:", Buk.main);

// Bun.which - Find executable
console.log("\nFinding 'node':", Buk.which("node") || "not found");
console.log("Finding 'cargo':", Buk.which("cargo") || "not found");

// Bun.file() - File operations
const testFile = Buk.file("test.txt");
console.log("\nFile operations:");
console.log("test.txt exists:", testFile.exists());

// Bun.write() - Write file
console.log("\nWriting to test_write.txt...");
const writeSuccess = Buk.write("test_write.txt", "Hello from Buk.write!");
console.log("Write success:", writeSuccess);

// Read it back
const writtenFile = Buk.file("test_write.txt");
console.log("Read back:", writtenFile.text());

// File size
console.log("File size:", writtenFile.size(), "bytes");

// Bun.sleep - Sleep (blocking, use sparingly)
console.log("\nSleeping for 100ms...");
console.time("sleep");
Buk.sleep(100);
console.timeEnd("sleep");

console.log("\n=== Demo Complete ===");
