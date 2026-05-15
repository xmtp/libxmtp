(ns xdbg-driver-lib.driver
  "Shared helpers for cross-test driver scripts. The cross-version-test
   and cross-talk-test drivers each have their own bb derivation but
   share the bulk of their plan-parsing / xdbg-invocation / summary code
   via this namespace, loaded from the lib's installed src dir through
   the driver's bb --classpath."
  (:require [babashka.fs :as fs]
            [babashka.process :as p]
            [cheshire.core :as json]
            [clojure.string :as str]
            [xdbg-driver-lib.xdbg :as xdbg]))

(defn eputs [& parts]
  (binding [*out* *err*]
    (apply println parts)))

;; Re-export short-sha so callers that already require this ns don't
;; need to add xdbg as a separate require just for the formatter.
(def short-sha xdbg/short-sha)

(defn pid []
  (.pid (java.lang.ProcessHandle/current)))

(defn ensure-dir! [path]
  (fs/create-dirs path))

(defn run-xdbg-info
  "Run xdbg with arbitrary args under the sandbox env. Emits ::group::/
   ::endgroup:: log markers around the invocation. env-extras is a
   {string string} map merged into the subprocess env."
  [env-extras kind sha & args]
  (let [short (short-sha sha)]
    (eputs (format "::group::xdbg@%s (info) %s" short (str/join " " args)))
    (let [env (merge env-extras (xdbg/sandbox-env-args env-extras))
          {:keys [exit]} (apply p/shell
                                {:continue true :extra-env env}
                                (concat (xdbg/xdbg-invocation kind sha) args))]
      (eputs "::endgroup::")
      exit)))

(defn run-xdbg-real
  "Run xdbg with `-b $BACKEND --json --fail-fast`. Returns the exit code.
   extra-flags is a vector of flags spliced in BEFORE the harness's own
   `-b ... --json --fail-fast` (used by cross-talk-test to inject
   --strict-versioning unconditionally). Tempdir naming uses tmp-prefix
   (e.g. \"xvt-call-\" or \"ctt-call-\")."
  [{:keys [env-extras tmp-prefix extra-flags base-dir]} kind sha & args]
  (let [short (short-sha sha)]
    (try
      (let [env (merge env-extras (xdbg/sandbox-env-args env-extras))
            backend (or (System/getenv "BACKEND")
                        (get env-extras "BACKEND")
                        "dev")
            xvt-flags (when-let [raw (or (get env-extras "XVT_XDBG_FLAGS")
                                         (System/getenv "XVT_XDBG_FLAGS"))]
                        (->> (str/split (str/trim raw) #"\s+")
                             (remove str/blank?)))
            xtra (cond-> (vec (concat (or extra-flags []) xvt-flags))
                   (xdbg/supports-flag kind sha "--trace-openmls-kv")
                   (conj "--trace-openmls-kv"))
            call-dir (str (fs/create-temp-dir {:prefix (or tmp-prefix "xvt-call-")
                                               :dir base-dir}))
            cmd (concat (xdbg/xdbg-invocation kind sha)
                        xtra ["-b" backend "--json" "--fail-fast"] args)
            {:keys [exit out err]}
            (apply p/shell {:out :string :err :string :continue true
                            :extra-env env :dir call-dir}
                   cmd)]
        (when (seq out) (print out) (flush))
        (when (seq err) (binding [*out* *err*] (print err) (flush)))
        (when-not (zero? exit)
          (eputs (format "::error::xdbg@%s failed: rc=%d" short exit)))
        exit)
      (catch Exception e
        (eputs (format "::error::xdbg@%s invocation failed: %s" short (.getMessage e)))
        1))))

(defn classify-kind
  "Classify a plan entry by its label + branch fields."
  [label branch]
  (let [label (or label "")
        branch (or branch "")]
    (cond
      (str/starts-with? label "nightly.") "nightly"
      (str/starts-with? label "HEAD") "head"
      (str/blank? label)
      (if (str/blank? branch) "head" "stable")
      :else
      (do (eputs (str "::warning::unrecognized plan label '" label
                      "'; treating as nightly"))
          "nightly"))))

(defn parse-plan-entry [raw]
  (let [role (:role raw)
        sha (:sha raw)
        short (or (:short raw) (when sha (short-sha sha)))
        branch (or (:branch raw) "")
        label (or (:label raw) "")
        kind (classify-kind label branch)]
    {:role role :short short :sha sha
     :branch branch :label label :kind kind
     :required? (not= kind "nightly")}))

(defn validate-entry
  "Check plan entry for required fields. Returns nil if valid, otherwise
   {:reason kw :entry entry} describing the problem. Caller decides how
   to render + whether to exit."
  [{:keys [role sha] :as entry}]
  (when (or (str/blank? (or role "")) (= role "null")
            (str/blank? (or sha "")) (= sha "null"))
    {:reason :missing-sha-or-role :entry entry}))

(defn read-plan
  "Read + parse plan.json. Returns one of:
     {:ok true :plan parsed-vector :count n}
     {:ok false :reason :parse-failed :message s}
     {:ok false :reason :too-few-entries :count n}
     {:ok false :reason :invalid-entry :entry e}

   Caller renders messages + decides exit codes (mechanism vs policy)."
  [plan-path]
  (let [raw (try
              {:ok? true :data (json/parse-string (slurp plan-path) true)}
              (catch Exception e
                {:ok? false :message (.getMessage e)}))]
    (if-not (:ok? raw)
      {:ok false :reason :parse-failed :message (:message raw)}
      (let [plan (:data raw)
            n (count plan)]
        (if (< n 2)
          {:ok false :reason :too-few-entries :count n}
          (let [parsed (mapv parse-plan-entry plan)
                bad (some validate-entry parsed)]
            (if bad
              {:ok false :reason :invalid-entry :entry (:entry bad)}
              {:ok true :plan parsed :count n})))))))

(def status->plain
  {"PASS"                "✓ PASS"
   "FAIL"                "✗ FAIL"
   "SKIP-BUILD"          "⊘ SKIP (build failed)"
   "SKIP-NOT-RUN"        "⊘ SKIP (not run — earlier required failure)"
   "SKIP-NO-HEALTHCHECK" "⊘ SKIP (no healthcheck subcommand)"})

(def status->md
  {"PASS"                "✅ PASS"
   "FAIL"                "❌ FAIL"
   "SKIP-BUILD"          "⊘ SKIP (build failed)"
   "SKIP-NOT-RUN"        "⊘ SKIP (not run — earlier required failure)"
   "SKIP-NO-HEALTHCHECK" "⊘ SKIP (no healthcheck subcommand)"})

(defn pipe-table->aligned
  "Reproduce `column -t -s '|'`: pad each pipe-separated cell to the
   widest cell in its column and join with two spaces."
  [lines]
  (let [rows (mapv #(str/split % #"\|" -1) lines)
        ncols (apply max 0 (map count rows))
        padded (mapv (fn [row]
                       (vec (concat row (repeat (- ncols (count row)) ""))))
                     rows)
        widths (vec (for [c (range ncols)]
                      (apply max 0 (map #(count (nth % c)) padded))))]
    (str/join "\n"
              (for [row padded]
                (str/trimr
                  (str/join "  "
                            (map-indexed
                              (fn [i cell]
                                (format (str "%-" (max 1 (nth widths i)) "s")
                                        cell))
                              row)))))))

(defn emit-stderr-table [test-name results]
  (let [header "STATUS|KIND|SHORT|SHA|LABEL"
        body
        (for [{:keys [status short sha kind branch label]} results]
          (let [glyph (get status->plain status (str "? " status))
                display (if (str/blank? (or label "")) (or branch "") label)]
            (str/join "|" [glyph kind short sha display])))]
    (eputs "")
    (eputs (str "=== " test-name " summary ==="))
    (binding [*out* *err*]
      (println (pipe-table->aligned (cons header body))))))

(defn emit-plan-table [parsed-entries]
  (let [header "ROLE|SHORT|SHA|BRANCH|LABEL"
        body (for [{:keys [role short sha branch label]} parsed-entries]
               (str/join "|" [role short sha branch label]))]
    (binding [*out* *err*]
      (println (pipe-table->aligned (cons header body))))))

(defn emit-plan-step-summary!
  "Append a ```json plan block to $GITHUB_STEP_SUMMARY (no-op when the
   env var is unset). Used by the `run` subcommand to make the plan
   visible on the workflow's run summary page."
  [test-name profile sample-size parsed-entries]
  (when-let [path (System/getenv "GITHUB_STEP_SUMMARY")]
    (let [body (str (format "## xdbg %s plan (%s, sample-size: %d)\n\n"
                            test-name profile (int sample-size))
                    "```json\n"
                    (json/generate-string parsed-entries {:pretty true})
                    "\n```\n")]
      (spit path body :append true))))

(defn write-plan-artifact!
  "Write the parsed plan as JSON to the CWD as plan.json so the CI
   workflow's upload-artifact step can capture it alongside the summary.
   No-op outside CI (caller checks GITHUB_OUTPUT presence — used as a
   proxy for 'running under GitHub Actions')."
  [parsed-entries]
  (when (System/getenv "GITHUB_OUTPUT")
    (spit "plan.json" (json/generate-string parsed-entries {:pretty true}))))

(defn format-summary-json
  "Hand-formatted JSON to match the bash version's exact whitespace.
   cheshire's pretty-printer arranges arrays differently. test-kind is
   an optional top-level field (cross-talk-test sets it; cross-version-
   test omits)."
  [{:keys [results completed-count required-failure nightly-failure
           lenient-nightlies test-kind]}]
  (let [test-kind-line (if test-kind
                         (format "\n  \"test_kind\": \"%s\"," test-kind)
                         "")
        head (format
               "{\n  \"completed_count\": %d,\n  \"nightly_failure\": %d,\n  \"required_failure\": %d,\n  \"lenient_nightlies\": %d,%s\n  \"results\": ["
               (int completed-count)
               (if nightly-failure 1 0)
               (if required-failure 1 0)
               (if lenient-nightlies 1 0)
               test-kind-line)
        rows (for [{:keys [status short sha kind branch label]} results]
               (format "    {\"status\":\"%s\",\"short\":\"%s\",\"sha\":\"%s\",\"kind\":\"%s\",\"branch\":\"%s\",\"label\":\"%s\"}"
                       status short sha kind (or branch "") (or label "")))]
    (str head
         (if (seq rows)
           (str "\n" (str/join ",\n" rows) "\n  ]\n}\n")
           "\n  ]\n}\n"))))

(defn count-ndjson-logs
  "Count NDJSON log files at depth <=1 below out-dir (root + immediate
   subdirectories), excluding summary.json. Deeper nesting isn't
   currently produced by the runners."
  [out-dir]
  (let [root (fs/path out-dir)]
    (if-not (fs/exists? root)
      0
      (count
        (filter (fn [p]
                  (let [name (str (fs/file-name p))]
                    (and (fs/regular-file? p)
                         (str/ends-with? name ".json")
                         (not= name "summary.json"))))
                (concat (fs/list-dir root)
                        (mapcat (fn [d]
                                  (when (fs/directory? d) (fs/list-dir d)))
                                (filter fs/directory? (fs/list-dir root)))))))))

(defn emit-github-step-summary
  [{:keys [test-name results out-dir completed-count required-failure
           nightly-failure lenient-nightlies]}]
  (when-let [path (System/getenv "GITHUB_STEP_SUMMARY")]
    (let [log-count (count-ndjson-logs out-dir)
          rows (for [{:keys [status short sha kind branch label]} results]
                 (let [glyph (get status->md status (str "❓ " status))
                       display (cond
                                 (seq label) label
                                 (seq branch) branch
                                 :else "-")]
                   (format "| %s | %s | `%s` | %s | `%s` |"
                           glyph kind short display sha)))
          body (str (format "## %s results\n\n" test-name)
                    "| Status | Kind | Short | Label / Branch | SHA |\n"
                    "|---|---|---|---|---|\n"
                    (str/join "\n" rows) "\n\n"
                    (format "**completed_count=%d required_failure=%d nightly_failure=%d lenient=%d**\n\n"
                            (int completed-count)
                            (if required-failure 1 0)
                            (if nightly-failure 1 0)
                            (if lenient-nightlies 1 0))
                    (format "**libxmtp NDJSON log files captured: %d** (download the run artifact to inspect)\n"
                            log-count))]
      (spit path body :append true))))

(defn record-not-run-remaining
  "Append SKIP-NOT-RUN entries for every parsed plan entry not yet
   processed, so the summary still lists every planned version."
  [results remaining-parsed]
  (into results
        (for [{:keys [short sha kind branch label]} remaining-parsed]
          {:status "SKIP-NOT-RUN" :short short :sha sha
           :kind kind :branch branch :label label})))

(defn setup-run-env!
  "Create the per-run XVT_DB_ROOT + XDBG_DRIVER_CACHE under tmp-prefix
   (e.g. \"xvt-\" or \"ctt-\"). Returns the env-extras map to merge into
   subsequent subprocess calls."
  [tmp-prefix]
  (let [backend (or (System/getenv "BACKEND") "dev")
        tmp (or (System/getenv "TMPDIR") "/tmp")
        xvt-db (str tmp "/" tmp-prefix "db-" (pid))
        driver-cache (str tmp "/xdbg-driver-cache-" (pid))]
    (ensure-dir! xvt-db)
    (spit driver-cache "")
    {"BACKEND" backend
     "XVT_DB_ROOT" xvt-db
     "XDBG_DRIVER_CACHE" driver-cache}))
