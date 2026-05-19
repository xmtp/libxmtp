(ns xdbg-driver-lib.xdbg
  (:require [babashka.process :as p]
            [babashka.fs :as fs]
            [clojure.string :as str]
            [clojure.java.io :as io]
            [xdbg-driver-lib.spinner :as spin]))

(def ^:dynamic *cache-file*
  (or (System/getenv "XDBG_DRIVER_CACHE")
      (str (or (System/getenv "TMPDIR") "/tmp")
           "/xdbg-driver-cache")))

;; ---------- Pure helpers ----------

(defn flake-ref
  "Build the flake reference for a (kind, sha) pair. For kind=head the
   local working-tree flake is used — resolved to an absolute `path:`
   ref (via GITHUB_WORKSPACE or the process's startup cwd) so callers
   can change subprocess cwd for tempdir isolation without breaking
   the relative `.#xdbg` lookup. For other kinds: github:xmtp/libxmtp/<sha>."
  [kind sha]
  (if (= kind "head")
    (let [root (or (System/getenv "GITHUB_WORKSPACE")
                   (System/getProperty "user.dir"))]
      (str "path:" root "#xdbg"))
    (str "github:xmtp/libxmtp/" sha "#xdbg")))

(defn sandbox-env-args
  "Return {env-name -> value} for the sandboxed xdbg invocation.
   When called with `env-extras` (the harness's per-run env map),
   prefer its XVT_DB_ROOT over the process env so in-process drivers
   that built `env-extras` directly still get the right value through
   to the xdbg child."
  ([] (sandbox-env-args nil))
  ([env-extras]
   {"XDBG_DB_ROOT" (or (get env-extras "XVT_DB_ROOT")
                       (System/getenv "XVT_DB_ROOT")
                       "")}))

;; ---------- Cache ----------
;;
;; Two-tier: in-process atom for the hot path (each driver run hits
;; supports-flag/-subcommand dozens of times), disk file for cross-
;; process persistence (CI workflow does bootstrap-fetch + pick-versions
;; + run-sequence as separate `nix run` invocations).

(def ^:private mem-cache (atom nil))

(defn- load-disk-cache!
  []
  (let [entries (if (fs/exists? *cache-file*)
                  (into {}
                        (for [line (str/split-lines (slurp *cache-file*))
                              :let [[k v] (str/split line #"\t" 2)]
                              :when (and k v)]
                          [k (try (Long/parseLong v) (catch Exception _ nil))]))
                  {})]
    (reset! mem-cache entries)
    entries))

(defn- cache-snapshot
  []
  (or @mem-cache (load-disk-cache!)))

(defn cache-lookup
  "Look up cached value for key. Returns 0/1 (cached) or nil (cache miss)."
  [key]
  (get (cache-snapshot) key))

(defn cache-store
  "Persist a (key, value) pair: update in-process atom + append to disk
   so other process invocations also benefit."
  [key value]
  (swap! mem-cache (fn [m] (assoc (or m {}) key value)))
  (io/make-parents *cache-file*)
  (spit *cache-file* (str key "\t" value "\n") :append true))

;; ---------- Subprocess helpers ----------

(defn short-sha
  "Abbreviate a sha to its first 7 chars (the conventional short form).
   Tolerates already-short input."
  [sha]
  (subs sha 0 (min 7 (count sha))))

(defn xdbg-invocation
  "Return the argv prefix to invoke xdbg for `(kind, sha)` via `nix run`.
   Callers append their own xdbg subcommand args, e.g.:
     (apply p/shell {…} (concat (xdbg-invocation kind sha) [\"--help\"]))
   Nix caches the build under (kind, sha) so subsequent invocations
   reuse the same store path — no need to memoize here. Sha-pinned
   flake refs are content-addressed; no --refresh required."
  [kind sha]
  ["nix" "run" "-L" (flake-ref kind sha) "--"])

(defn probe
  "Probe whether xdbg is exposed and (optionally) builds. Returns a map:
     {:status :ok}
     {:status :build-failed :stderr s}    — compile error, etc.
     {:status :eval-failed  :stderr s :exit n} — flake attr missing, network, etc.

   Caller decides how to render/log the failure (probe is mechanism;
   GitHub-Actions log markers are policy)."
  [mode kind sha]
  (let [nix-args (cond-> ["nix" "build" "-L"]
                   (= mode "dry") (conj "--dry-run")
                   :always (conj (flake-ref kind sha)))
        label (str "probing xdbg@" (short-sha sha) " (" kind
                   (when (= mode "dry") ", dry") ")")
        {:keys [exit err]}
        (spin/with-spinner label
          #(apply p/shell {:err :string :continue true} nix-args))]
    (cond
      (zero? exit) {:status :ok}
      (re-find #"builder( for '[^']*')? failed|build of '[^']*' failed|builder failed with exit code" err)
      {:status :build-failed :stderr err}
      :else
      {:status :eval-failed :stderr err :exit exit})))

(defn- run-help
  "Invoke xdbg with --help and return {:exit :out}."
  [kind sha]
  (try
    (let [env (sandbox-env-args)
          {:keys [exit out]}
          (apply p/shell
                 {:out :string :continue true :extra-env env}
                 (concat (xdbg-invocation kind sha) ["--help"]))]
      {:exit exit :out out})
    (catch Exception _
      {:exit 1 :out ""})))

(defn supports-flag
  "Check whether xdbg --help advertises <flag>. Cached by (kind, sha, flag)
   on disk in *cache-file*."
  [kind sha flag]
  (let [key (str kind ":" sha ":flag:" flag)]
    (if-let [cached (cache-lookup key)]
      (zero? cached)
      (let [{:keys [exit out]} (run-help kind sha)
            supported? (and (zero? exit) (str/includes? out flag))]
        (cache-store key (if supported? 0 1))
        supported?))))

(defn supports-subcommand
  "Check whether `xdbg <sub> --help` succeeds. Cached."
  [kind sha sub]
  (let [key (str kind ":" sha ":sub:" sub)]
    (if-let [cached (cache-lookup key)]
      (zero? cached)
      (let [supported?
            (try
              (let [env (sandbox-env-args)
                    {:keys [exit]} (apply p/shell
                                          {:out nil :err nil :continue true
                                           :extra-env env}
                                          (concat (xdbg-invocation kind sha)
                                                  [sub "--help"]))]
                (zero? exit))
              (catch Exception _ false))]
        (cache-store key (if supported? 0 1))
        supported?))))
