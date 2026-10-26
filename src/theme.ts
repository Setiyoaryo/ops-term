import type { ITheme } from "@xterm/xterm";

/** Terminal color schemes. All original palettes (PRD section 4.3). */
export const TERMINAL_THEMES: Record<string, ITheme> = {
  "default-dark": {
    background: "#111418",
    foreground: "#E5E9EF",
    cursor: "#4F8EE8",
    selectionBackground: "#2B323A",
    black: "#111418",
    red: "#E06C75",
    green: "#98C379",
    yellow: "#E5C07B",
    blue: "#4F8EE8",
    magenta: "#C678DD",
    cyan: "#56B6C2",
    white: "#E5E9EF",
    brightBlack: "#5C6370",
    brightRed: "#E06C75",
    brightGreen: "#98C379",
    brightYellow: "#E5C07B",
    brightBlue: "#61AFEF",
    brightMagenta: "#C678DD",
    brightCyan: "#56B6C2",
    brightWhite: "#FFFFFF",
  },
  "default-light": {
    background: "#FAFAF7",
    foreground: "#24292F",
    cursor: "#0969DA",
    selectionBackground: "#C8E1FF",
    black: "#24292F",
    red: "#CF222E",
    green: "#116329",
    yellow: "#7D4E00",
    blue: "#0969DA",
    magenta: "#8250DF",
    cyan: "#1B7C83",
    white: "#6E7781",
    brightBlack: "#57606A",
    brightRed: "#A40E26",
    brightGreen: "#1A7F37",
    brightYellow: "#9A6700",
    brightBlue: "#218BFF",
    brightMagenta: "#A475F9",
    brightCyan: "#3192AA",
    brightWhite: "#24292F",
  },
  "solarized-dark": {
    background: "#002B36",
    foreground: "#93A1A1",
    cursor: "#93A1A1",
    selectionBackground: "#073642",
    black: "#073642",
    red: "#DC322F",
    green: "#859900",
    yellow: "#B58900",
    blue: "#268BD2",
    magenta: "#D33682",
    cyan: "#2AA198",
    white: "#EEE8D5",
    brightBlack: "#002B36",
    brightRed: "#CB4B16",
    brightGreen: "#586E75",
    brightYellow: "#657B83",
    brightBlue: "#839496",
    brightMagenta: "#6C71C4",
    brightCyan: "#93A1A1",
    brightWhite: "#FDF6E3",
  },
  "classic-green": {
    background: "#000000",
    foreground: "#33FF33",
    cursor: "#33FF33",
    selectionBackground: "#0A3A0A",
    black: "#000000",
    red: "#33FF33",
    green: "#33FF33",
    yellow: "#33FF33",
    blue: "#33FF33",
    magenta: "#33FF33",
    cyan: "#33FF33",
    white: "#33FF33",
    brightBlack: "#1E8C1E",
    brightRed: "#66FF66",
    brightGreen: "#66FF66",
    brightYellow: "#66FF66",
    brightBlue: "#66FF66",
    brightMagenta: "#66FF66",
    brightCyan: "#66FF66",
    brightWhite: "#CCFFCC",
  },
};

export const COLOR_SCHEME_OPTIONS = [
  { value: "default-dark", label: "Default Dark" },
  { value: "default-light", label: "Default Light" },
  { value: "solarized-dark", label: "Solarized Dark" },
  { value: "classic-green", label: "Classic Green" },
];

export function terminalTheme(name: string): ITheme {
  return TERMINAL_THEMES[name] ?? TERMINAL_THEMES["default-dark"];
}

/** Whether the UI should render its dark chrome. */
export function isDarkUi(uiTheme: string): boolean {
  if (uiTheme === "system") {
    return !window.matchMedia("(prefers-color-scheme: light)").matches;
  }
  return uiTheme !== "light";
}
