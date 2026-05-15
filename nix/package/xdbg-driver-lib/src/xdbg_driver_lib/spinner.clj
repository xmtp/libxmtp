(ns xdbg-driver-lib.spinner
  "Local-only progress spinner. Wraps a thunk and prints a rotating
   glyph on stderr while it runs. Suppressed in CI and when stderr
   isn't a TTY so logs in GitHub Actions / piped output stay clean.

   Mechanism, not policy: callers pass their own label; this ns
   doesn't know what's being done."
  (:require [clojure.string :as str]))

;; ---------- Detection ----------

(def ^:private frames ["⠋" "⠙" "⠹" "⠸" "⠼" "⠴" "⠦" "⠧" "⠇" "⠏"])

(defn- tty?
  "Return true when stderr looks like an interactive terminal. In
   GraalVM-compiled babashka, `System/console` returns nil under
   redirected stderr — same behavior the C runtime gives bash's
   `[ -t 2 ]`. CI environments also tend to set CI=true or redirect."
  []
  (and (some? (System/console))
       (str/blank? (or (System/getenv "CI") ""))
       (str/blank? (or (System/getenv "GITHUB_ACTIONS") ""))))

;; ---------- Rendering ----------

(defn- write-stderr! [s]
  (binding [*out* *err*]
    (print s)
    (flush)))

(defn- clear-line! []
  ;; \r returns cursor; \033[2K clears the entire line. Most terminals
  ;; (including CI's "fake" pty) honor this so even if we mis-detect a
  ;; TTY, leftover frames don't persist.
  (write-stderr! "\r\033[2K"))

(def ^:private first-frame-delay-ms
  "Sleep before painting the first frame. Operations that complete in
   under this budget (cached nix builds, no-op fetches) render nothing
   at all — no flicker."
  500)

(defn- spinner-loop!
  "Background loop: sleep first-frame-delay-ms, then write a frame
   every 100ms until `running?` flips false. Each frame overwrites the
   previous via \\r. Returns true if any frame was written, false if
   `f` finished before the first frame would have painted (so callers
   can skip the clear-line emit)."
  [label running?]
  (Thread/sleep first-frame-delay-ms)
  (loop [i 0
         painted? false]
    (if @running?
      (do (write-stderr! (str "\r" (nth frames (mod i (count frames))) " " label))
          (Thread/sleep 100)
          (recur (inc i) true))
      painted?)))

;; ---------- Public API ----------

(defn with-spinner
  "Run `f` while displaying a spinner on stderr labeled `label`. The
   spinner is fully suppressed when stderr isn't an interactive
   terminal or when CI / GITHUB_ACTIONS is set, so callers don't need
   to branch on environment themselves.

   Returns whatever `f` returns. If `f` throws, the spinner is still
   cleared so the exception's stack trace renders cleanly."
  [label f]
  (if-not (tty?)
    (f)
    (let [running? (atom true)
          fut (future (spinner-loop! label running?))]
      (try
        (f)
        (finally
          (reset! running? false)
          (try
            (when @fut
              (clear-line!))
            (catch Exception _)))))))
