// Service worker LinguaRust: офлайн-режим для статической версии.
//
// Стратегия: cache-first с фоновым обновлением (stale-while-revalidate).
// При изменении версии ниже старые кэши удаляются, поэтому правки
// словаря и текстов попадают на устройство при следующем запуске.

const VERSION = "linguarust-v1";
const SHELL = [
  "./",
  "index.html",
  "app.js",
  "style.css",
  "icon.svg",
  "manifest.webmanifest",
  "data/dictionary.tsv",
  "data/texts.json",
  "data/grammar.json",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(VERSION)
      .then((cache) => cache.addAll(SHELL))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter((key) => key !== VERSION).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  event.respondWith(
    caches.match(request).then((cached) => {
      const network = fetch(request)
        .then((response) => {
          if (response && response.status === 200) {
            const copy = response.clone();
            caches.open(VERSION).then((cache) => cache.put(request, copy));
          }
          return response;
        })
        .catch(() => cached);

      return cached || network;
    }),
  );
});
