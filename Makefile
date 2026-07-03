.PHONY: dev start backend frontend install test test-backend test-frontend build build-frontend help

dev: start

start:
	./scripts/dev.sh

backend:
	cargo run -p prudentia-backend

frontend:
	npm --prefix frontend run dev

install:
	npm install --prefix frontend

test: test-backend test-frontend

test-backend:
	cargo test -p prudentia-backend

test-frontend:
	npm --prefix frontend test

build: build-frontend

build-frontend:
	npm --prefix frontend run build

help:
	@printf '%s\n' \
		'Targets:' \
		'  make start          Start backend and frontend' \
		'  make backend        Start only the backend' \
		'  make frontend       Start only the frontend' \
		'  make install        Install frontend dependencies' \
		'  make test           Run backend and frontend tests' \
		'  make build          Build the frontend'
