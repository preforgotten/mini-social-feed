.PHONY: run build clean seed

# Start the demo server (uses existing data)
run: build
	@echo "Starting demo on http://localhost:3000 ..."
	@cargo run

# Build the frontend
build:
	@if [ ! -d ../frontend/node_modules ]; then \
		echo "Installing frontend deps..."; \
		cd ../frontend && npm install; \
	fi
	@echo "Building frontend..."
	@cd ../frontend && npm run build

# Clean all data and start fresh with seed
seed: clean run

# Clean demo data and uploads
clean:
	rm -rf data uploads
	@echo "Demo data cleared."
