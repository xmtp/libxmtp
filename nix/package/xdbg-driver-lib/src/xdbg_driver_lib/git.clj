(ns xdbg-driver-lib.git
  (:require [babashka.process :as p]
            [clojure.string :as str]
            [xdbg-driver-lib.spinner :as spin]))

(defn- git-call
  "Run git with args, returning {:exit :out :err}. :continue true so
   caller decides how to react."
  [& args]
  (apply p/shell {:out :string :err :string :continue true} "git" args))

(defn for-each-ref
  "List ref names matching pattern, one per line, blanks dropped."
  [pattern]
  (let [{:keys [exit out err]}
        (git-call "for-each-ref" "--format=%(refname:short)" pattern)]
    (when-not (zero? exit)
      (throw (ex-info "git for-each-ref failed"
                      {:pattern pattern :exit exit :stderr err})))
    (->> (str/split-lines out)
         (remove str/blank?))))

(defn tags-by-creator-date
  "All tags in repo, sorted by creator date descending. Blanks dropped."
  []
  (let [{:keys [exit out err]} (git-call "tag" "--sort=-creatordate")]
    (when-not (zero? exit)
      (throw (ex-info "git tag failed" {:exit exit :stderr err})))
    (->> (str/split-lines out)
         (remove str/blank?))))

(defn rev-parse
  "Resolve a ref to its full sha. Throws on unknown ref."
  [ref]
  (let [{:keys [exit out err]} (git-call "rev-parse" ref)]
    (when-not (zero? exit)
      (throw (ex-info "git rev-parse failed"
                      {:ref ref :exit exit :stderr err})))
    (str/trim out)))

(defn rev-parse-multi
  "Resolve multiple refs in a single git call. Returns vector of shas.
   Throws if any ref is unknown."
  [& refs]
  (let [{:keys [exit out err]} (apply git-call "rev-parse" refs)]
    (when-not (zero? exit)
      (throw (ex-info "git rev-parse failed"
                      {:refs refs :exit exit :stderr err})))
    (->> (str/split-lines out)
         (map str/trim)
         vec)))

(defn rev-parse-verify
  "Resolve a commit-ish to its full sha, returning nil on unknown ref.
   Unlike rev-parse, never throws on unknown — used for nightly resolution."
  [commitish]
  (let [{:keys [exit out]} (git-call "rev-parse" "--verify" commitish)]
    (when (zero? exit)
      (str/trim out))))

(defn abbrev-ref-head
  "Return the current ref name (e.g. \"main\") or nil if detached HEAD."
  []
  (let [{:keys [exit out]} (git-call "rev-parse" "--abbrev-ref" "HEAD")]
    (when (zero? exit)
      (let [trimmed (str/trim out)]
        (when (and (seq trimmed) (not= trimmed "HEAD"))
          trimmed)))))

;; Binding families we publish nightly tags for. Excluded by design: cli,
;; kotlin, swift, and per-commit ios tags — they explode tag count
;; (~1500 vs ~50 for the listed families) without adding cross-version
;; signal.
(def ^:private nightly-tag-regex
  #"^(?:node-bindings|wasm-bindings|android|ios)-.*-nightly\.\d{8}\.[0-9a-f]{7}$")

(defn filter-nightly-tags
  "Given lines from `git ls-remote --tags --refs origin` (or any list of
   tag-like strings, with or without `refs/tags/` prefix), return the
   subset that matches the nightly-binding pattern.

   Filtering out non-nightly tags is the key optimization — the repo has
   ~1500 cli/kotlin/swift/ios per-commit tags that dominate fetch time."
  [lines]
  (->> lines
       (keep (fn [line]
               (let [name (-> line
                              (str/replace #".*\trefs/tags/" "")  ; strip ls-remote prefix
                              (str/replace #"^refs/tags/" "")    ; or bare prefix
                              str/trim)]
                 (when (re-matches nightly-tag-regex name)
                   name))))))

(defn- run-git-fetch [& args]
  (let [{:keys [exit err]} (apply p/shell {:err :string :continue true} "git" "fetch" args)]
    (when-not (zero? exit)
      (throw (ex-info "git fetch failed"
                      {:args args :exit exit :stderr err})))))

(defn bootstrap-fetch
  "Fetch release branches + nightly tags into the local repo so
   `pick-versions` can resolve them. Idempotent — re-running is cheap.

   Returns {:branches-fetched true :nightly-tags-fetched N}. Caller
   decides how to render warnings/notices (mechanism vs policy)."
  []
  (spin/with-spinner "fetching release branches"
    #(run-git-fetch "origin"
                    "+refs/heads/release/*:refs/remotes/origin/release/*"
                    "--no-tags" "--depth=1"))
  (let [{:keys [exit out err]}
        (spin/with-spinner "listing remote tags"
          #(p/shell {:out :string :err :string :continue true}
                    "git" "ls-remote" "--tags" "--refs" "origin"))
        _ (when-not (zero? exit)
            (throw (ex-info "git ls-remote failed" {:exit exit :stderr err})))
        tags (filter-nightly-tags (str/split-lines out))]
    (when (seq tags)
      (let [refspecs (map #(str "refs/tags/" % ":refs/tags/" %) tags)]
        (spin/with-spinner (format "fetching %d nightly tags" (count tags))
          #(apply run-git-fetch "origin" "--no-tags" "--depth=1" refspecs))))
    {:branches-fetched true
     :nightly-tags-fetched (count tags)}))
