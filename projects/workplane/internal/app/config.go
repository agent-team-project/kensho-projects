package app

import (
	"fmt"
	"os"
	"strings"
	"time"
)

type Config struct {
	DatabaseURL          string
	PublicOrigin         string
	TokenHashKey         []byte
	SessionTTL           time.Duration
	CookieSecure         bool
	Bootstrap            *BootstrapConfig
	FaultInjection       bool
	RealtimePollInterval time.Duration
	RealtimeHeartbeat    time.Duration
	RealtimeCursorTTL    time.Duration
	RealtimeWriteTimeout time.Duration
	RealtimeMaxUnacked   int
	RealtimeBuffer       int
}

type BootstrapConfig struct {
	OrganizationID   string
	OrganizationSlug string
	OrganizationName string
	HumanID          string
	HumanName        string
	HumanEmail       string
	HumanPassword    string
	AgentID          string
	AgentName        string
	AgentToken       string
}

func ConfigFromEnv() (Config, error) {
	cfg := Config{
		DatabaseURL:          os.Getenv("WORKPLANE_DATABASE_URL"),
		PublicOrigin:         strings.TrimRight(os.Getenv("WORKPLANE_PUBLIC_ORIGIN"), "/"),
		TokenHashKey:         []byte(os.Getenv("WORKPLANE_TOKEN_HASH_KEY")),
		SessionTTL:           8 * time.Hour,
		CookieSecure:         os.Getenv("WORKPLANE_COOKIE_SECURE") != "false",
		FaultInjection:       os.Getenv("WORKPLANE_FAULT_INJECTION") == "true",
		RealtimePollInterval: 25 * time.Millisecond,
		RealtimeHeartbeat:    time.Second,
		RealtimeCursorTTL:    24 * time.Hour,
		RealtimeWriteTimeout: time.Second,
		RealtimeMaxUnacked:   8,
		RealtimeBuffer:       8,
	}
	if cfg.PublicOrigin == "" {
		cfg.PublicOrigin = "http://localhost:8080"
	}
	if cfg.DatabaseURL == "" {
		return Config{}, fmt.Errorf("WORKPLANE_DATABASE_URL is required")
	}
	if len(cfg.TokenHashKey) < 32 {
		return Config{}, fmt.Errorf("WORKPLANE_TOKEN_HASH_KEY must contain at least 32 bytes")
	}
	if os.Getenv("WORKPLANE_BOOTSTRAP_ORG_ID") != "" {
		cfg.Bootstrap = &BootstrapConfig{
			OrganizationID:   os.Getenv("WORKPLANE_BOOTSTRAP_ORG_ID"),
			OrganizationSlug: envOr("WORKPLANE_BOOTSTRAP_ORG_SLUG", "walking-slice"),
			OrganizationName: envOr("WORKPLANE_BOOTSTRAP_ORG_NAME", "Walking Slice"),
			HumanID:          os.Getenv("WORKPLANE_BOOTSTRAP_HUMAN_ID"),
			HumanName:        envOr("WORKPLANE_BOOTSTRAP_HUMAN_NAME", "Walking Slice Human"),
			HumanEmail:       strings.ToLower(os.Getenv("WORKPLANE_BOOTSTRAP_HUMAN_EMAIL")),
			HumanPassword:    os.Getenv("WORKPLANE_BOOTSTRAP_HUMAN_PASSWORD"),
			AgentID:          os.Getenv("WORKPLANE_BOOTSTRAP_AGENT_ID"),
			AgentName:        envOr("WORKPLANE_BOOTSTRAP_AGENT_NAME", "Walking Slice Agent"),
			AgentToken:       os.Getenv("WORKPLANE_BOOTSTRAP_AGENT_TOKEN"),
		}
		if cfg.Bootstrap.HumanID == "" || cfg.Bootstrap.HumanEmail == "" ||
			cfg.Bootstrap.HumanPassword == "" || cfg.Bootstrap.AgentID == "" ||
			len(cfg.Bootstrap.AgentToken) < 32 {
			return Config{}, fmt.Errorf("bootstrap identity variables are incomplete")
		}
	}
	return cfg, nil
}

func envOr(name, fallback string) string {
	if value := os.Getenv(name); value != "" {
		return value
	}
	return fallback
}
