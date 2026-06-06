import { useEffect, useState } from "react";

export function useStoredList(key: string) {
  const [items, setItems] = useState<string[]>(() => readList(key));
  useEffect(() => {
    localStorage.setItem(key, JSON.stringify(items));
  }, [items, key]);
  const toggle = (value: string) => {
    setItems((current) => current.includes(value) ? current.filter((item) => item !== value) : [...current, value]);
  };
  return { items, toggle };
}

export function useStoredBoolean(key: string, fallback = false) {
  const [value, setValue] = useState(() => localStorage.getItem(key) === null ? fallback : localStorage.getItem(key) === "true");
  useEffect(() => {
    localStorage.setItem(key, String(value));
  }, [key, value]);
  return [value, setValue] as const;
}

function readList(key: string) {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}
