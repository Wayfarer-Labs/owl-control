import React, { createContext, useContext, useEffect, useState } from "react";

type Theme = "dark" | "light" | "system";

type ThemeProviderProps = {
  children: React.ReactNode;
  defaultTheme?: Theme;
  storageKey?: string;
};

type ThemeProviderState = {
  theme: Theme;
  setTheme: (theme: Theme) => void;
};

const initialState: ThemeProviderState = {
  theme: "system",
  setTheme: () => null,
};

const ThemeProviderContext = createContext<ThemeProviderState>(initialState);

export function ThemeProvider({
  children,
  defaultTheme = "dark",
  storageKey = "vg-theme",
  ...props
}: ThemeProviderProps) {
  // Always use dark theme
  const [theme] = useState<Theme>("dark");

  useEffect(() => {
    // Force dark theme
    const root = window.document.documentElement;
    root.classList.remove("light", "system");
    root.classList.add("dark");
  }, []);

  const value: ThemeProviderState = {
    theme: "dark",
    setTheme: () => {
      // Do nothing - always dark theme
      console.log("Theme is locked to dark mode");
    },
  };

  return (
    <ThemeProviderContext.Provider {...props} value={value}>
      {children}
    </ThemeProviderContext.Provider>
  );
}

export const useTheme = () => {
  const context = useContext(ThemeProviderContext);

  if (context === undefined)
    throw new Error("useTheme must be used within a ThemeProvider");

  return context;
};
