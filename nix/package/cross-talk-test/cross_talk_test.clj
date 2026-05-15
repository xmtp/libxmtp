#!/usr/bin/env bb
;; cross-talk-test — drive multiple xdbg builds against a shared
;; XDBG_DB_ROOT, but with each version running under --strict-versioning
;; so identities (and the SQLite files behind them) are partitioned per
;; version. Tests wire-level MLS interop: a v1.9 identity and a v1.10
;; identity in the same group can still send + receive messages.
;;
;; Usage:
;;   cross-talk-test run [--profile stable|nightly] [--sample-size N]
;;                       [-- xdbg-flags...]

(ns cross-talk-test
  (:require [babashka.cli :as cli]
            [babashka.fs :as fs]
            [babashka.process :as p]
            [cheshire.core :as json]
            [clojure.string :as str]
            [xdbg-driver-lib.driver :as d]
            [xdbg-driver-lib.git :as git]
            [xdbg-driver-lib.versions :as v]
            [xdbg-driver-lib.xdbg :as xdbg]))

(defn- finalize!
  [{:keys [results completed-count required-failure nightly-failure
           lenient-nightlies out-dir]}]
  (d/emit-stderr-table "cross-talk-test" results)
  (let [summary-json (str out-dir "/summary.json")]
    (spit summary-json
          (d/format-summary-json
            {:results results
             :completed-count completed-count
             :required-failure required-failure
             :nightly-failure nightly-failure
             :lenient-nightlies lenient-nightlies
             :test-kind "cross-talk"}))
    (d/eputs (str "::notice::summary JSON: " summary-json)))
  (d/emit-github-step-summary
    {:test-name "cross-talk-test"
     :results results :out-dir out-dir
     :completed-count completed-count
     :required-failure required-failure
     :nightly-failure nightly-failure
     :lenient-nightlies lenient-nightlies})
  (cond
    (< completed-count 2)
    (do (d/eputs (str "::error::insufficient cross-talk coverage (completed="
                      completed-count "); need >=2"))
        (System/exit 1))
    required-failure
    (do (d/eputs "::error::required version failed cross-talk healthcheck")
        (System/exit 1))
    (and nightly-failure (not lenient-nightlies))
    (do (d/eputs "::error::nightly failure (strict mode)")
        (System/exit 1))
    :else
    (d/eputs "::notice::cross-talk-test completed")))

(defn- run-strict*
  "cross-talk variant: always prepend --strict-versioning. Returns the
   full {:exit :out :err}. When tee? is true (default), stdout/stderr
   are also written through to the harness's stdout/stderr; when false
   the caller wants the captured output unpolluted (e.g. parsing JSON)."
  [{:keys [tee?] :or {tee? true}} env-extras out-dir kind sha args]
  (let [short (d/short-sha sha)]
    (try
      (let [env (merge env-extras (xdbg/sandbox-env-args env-extras))
            backend (or (System/getenv "BACKEND")
                        (get env-extras "BACKEND")
                        "dev")
            xvt-flags (when-let [raw (or (get env-extras "XVT_XDBG_FLAGS")
                                         (System/getenv "XVT_XDBG_FLAGS"))]
                        (->> (str/split (str/trim raw) #"\s+")
                             (remove str/blank?)))
            xtra (cond-> ["--strict-versioning"]
                   (seq xvt-flags) (into xvt-flags)
                   (xdbg/supports-flag kind sha "--trace-openmls-kv")
                   (conj "--trace-openmls-kv"))
            call-dir (str (fs/create-temp-dir {:prefix "ctt-call-" :dir out-dir}))
            cmd (concat (xdbg/xdbg-invocation kind sha)
                        xtra ["-b" backend "--json" "--fail-fast"] args)
            {:keys [exit out err] :as result}
            (apply p/shell {:out :string :err :string :continue true
                            :extra-env env :dir call-dir}
                   cmd)]
        (when tee?
          (when (seq out) (print out) (flush))
          (when (seq err) (binding [*out* *err*] (print err) (flush))))
        (when-not (zero? exit)
          (d/eputs (format "::error::xdbg@%s failed: rc=%d" short exit)))
        result)
      (catch Exception e
        (d/eputs (format "::error::xdbg@%s invocation failed: %s"
                         short (.getMessage e)))
        {:exit 1 :out "" :err ""}))))

(defn- run-strict
  "Tee'ing variant — emits xdbg's output as a side effect. Use for the
   phases whose output the user reads in the GitHub-Actions log."
  [env-extras out-dir kind sha & args]
  (run-strict* {:tee? true} env-extras out-dir kind sha args))

(defn- run-strict-quiet
  "Non-tee'ing variant — captures output without printing it. Use when
   the caller needs to parse stdout (e.g. `export -e group`)."
  [env-extras out-dir kind sha & args]
  (run-strict* {:tee? false} env-extras out-dir kind sha args))

(defn- preflight!
  "Probe + check --strict-versioning + sync subcommand. Exit 1 on any
   failure; cross-talk doesn't have a SKIP-NOT-RUN fallback."
  [env-extras {:keys [kind sha short]}]
  (let [mode (if (= kind "nightly") "full" "dry")
        {:keys [status stderr]} (xdbg/probe mode kind sha)]
    (when-not (= :ok status)
      (when (seq stderr)
        (binding [*out* *err*] (print stderr) (flush)))
      (d/eputs (str "::error::probe failed for " short " (" kind ") status=" status))
      (System/exit 1)))
  (when-not (xdbg/supports-flag kind sha "--strict-versioning")
    (d/eputs (str "::error::xdbg@" short
                  " lacks --strict-versioning; cannot participate in cross-talk-test"))
    (System/exit 1))
  (when-not (xdbg/supports-subcommand kind sha "sync")
    (d/eputs (str "::error::xdbg@" short
                  " lacks 'sync' subcommand; cannot participate in cross-talk-test"))
    (System/exit 1))
  (d/run-xdbg-info env-extras kind sha "--version"))

(defn- sync!
  "sync everyones client"
  [env-extras out-dir parsed]
  (d/eputs "::group::cross-talk sync all clients")
  (doseq [{:keys [kind sha]} parsed]
    (run-strict env-extras out-dir kind sha "sync"))
  (d/eputs "::endgroup::"))


(defn- phase1-bootstrap!
  "each version creates an identity"
  [env-extras out-dir parsed oldest-idx]
  (d/eputs "::group::cross-talk phase 1: bootstrap identities")
  (doseq [[i {:keys [kind sha]}] (map-indexed vector parsed)
          :let [n (if (= i oldest-idx) 3 1)]
          _ (range n)]
    (run-strict env-extras out-dir kind sha
                "generate" "-e" "identity" "--amount" "1"))
  (d/eputs "::endgroup::"))

(defn- phase2-create-group!
  "oldest creates group"
  [env-extras out-dir oldest]
  (d/eputs "::group::cross-talk phase 2: oldest creates group")
  (run-strict env-extras out-dir (:kind oldest) (:sha oldest)
              "generate" "-e" "group" "--amount" "1")
  (let [{:keys [exit out]} (run-strict-quiet env-extras out-dir
                                             (:kind oldest) (:sha oldest)
                                             "export" "-e" "group")
        groups (when (and (zero? exit) (seq out))
                 (try (json/parse-string out true)
                      (catch Exception _ nil)))
        gid (some-> groups first :id)]
    (if (str/blank? gid)
      (do (d/eputs "::error::could not capture group_id from `xdbg export -e group`; aborting")
          (d/eputs "::endgroup::")
          (System/exit 1))
      (do (d/eputs (str "::notice::shared group_id=" gid))
          (d/eputs "::endgroup::")
          gid))))

(defn- phase3-oldest-adds-joiners!
  "oldest adds newer versions"
  [env-extras out-dir oldest gid]
  (d/eputs "::group::cross-talk phase 3: oldest adds joiner identities + promotes")
  (run-strict env-extras out-dir (:kind oldest) (:sha oldest)
              "modify" "add-from-redb" gid
              "--include-versions" "other" "--promote-super-admin")
  (d/eputs "::endgroup::"))

(defn- phase4-healthcheck!
  [env-extras out-dir parsed]
  (d/eputs "::group::cross-talk phase 7: healthcheck")
  (let [result
        (reduce
          (fn [acc {:keys [kind sha short branch label]}]
            (let [base-row {:short short :sha sha :kind kind
                            :branch branch :label label}
                  {:keys [exit]} (run-strict env-extras out-dir kind sha "healthcheck")]
              (if (zero? exit)
                (-> acc
                    (update :results conj (assoc base-row :status "PASS"))
                    (update :completed-count inc))
                (cond-> (update acc :results conj
                                (assoc base-row :status "FAIL"))
                  (= kind "nightly") (assoc :nightly-failure? true)
                  (not= kind "nightly") (assoc :required-failure? true)))))
          {:results [] :completed-count 0
           :required-failure? false :nightly-failure? false}
          parsed)]
    (d/eputs "::endgroup::")
    result))

(defn- run-sequence-body
  [env-extras parsed out-dir lenient-nightlies]
  (let [_ (d/eputs (str "::notice::Running " (count parsed) " versions"))]
    (d/emit-plan-table parsed)
    (doseq [entry parsed] (preflight! env-extras entry))
    (let [oldest-idx 0
          oldest (nth parsed oldest-idx)]
      (phase1-bootstrap! env-extras out-dir parsed oldest-idx)
      (let [gid (phase2-create-group! env-extras out-dir oldest)]
        (phase3-oldest-adds-joiners! env-extras out-dir oldest gid)
        (sync! env-extras out-dir parsed)
        (let [{:keys [results completed-count
                      required-failure? nightly-failure?]}
              (phase4-healthcheck! env-extras out-dir parsed)]
          (finalize! {:results results
                      :completed-count completed-count
                      :required-failure required-failure?
                      :nightly-failure nightly-failure?
                      :lenient-nightlies lenient-nightlies
                      :out-dir out-dir}))))))

(defn- default-sample-size [profile]
  (case profile "stable" 0 "nightly" 3))

(defn- bootstrap-and-pick!
  "Run git bootstrap-fetch then build the plan in-process."
  [sample-size]
  (let [{:keys [nightly-tags-fetched]} (git/bootstrap-fetch)]
    (binding [*out* *err*]
      (if (zero? nightly-tags-fetched)
        (println "::warning::no nightly tags matched; nightly sampling will be empty")
        (println (format "Fetching %d nightly tags" nightly-tags-fetched)))))
  (v/pick-versions {:sample-size sample-size}))

(defn cmd-run
  "Pick a plan + execute the cross-talk sequence. Opts: :profile,
   :sample-size. Trailing args after `--` are forwarded to every xdbg
   invocation via XVT_XDBG_FLAGS."
  [{:keys [opts args]}]
  (let [{:keys [profile sample-size]} opts
        sample-size (or sample-size (default-sample-size profile))
        lenient? (= profile "nightly")
        xdbg-flags (str/join " " args)
        parsed (mapv d/parse-plan-entry (bootstrap-and-pick! sample-size))
        bad (some d/validate-entry parsed)]
    (when bad
      (d/eputs (str "cross-talk-test: plan entry missing sha or role: "
                    (:entry bad)))
      (System/exit 2))
    (when (< (count parsed) 2)
      (d/eputs (str "cross-talk-test: plan must have at least 2 entries; got "
                    (count parsed)))
      (System/exit 2))
    (d/emit-plan-step-summary! "cross-talk" profile sample-size parsed)
    (d/write-plan-artifact! parsed)
    (let [out-dir (or (System/getenv "XVT_OUT_DIR")
                      (str (or (System/getenv "TMPDIR") "/tmp")
                           "/ctt-out-" (d/pid)))]
      (d/ensure-dir! out-dir)
      (when-let [gh-out (System/getenv "GITHUB_OUTPUT")]
        (spit gh-out (str "out_dir=" out-dir "\n") :append true))
      (let [env-extras (cond-> (d/setup-run-env! "ctt-")
                         (seq xdbg-flags) (assoc "XVT_XDBG_FLAGS" xdbg-flags))]
        (d/eputs (str "::notice::cross-talk-test XDBG_DB_ROOT="
                      (get env-extras "XVT_DB_ROOT")))
        (d/eputs (str "::notice::cross-talk-test out_dir=" out-dir))
        (when (seq xdbg-flags)
          (d/eputs (str "::notice::cross-talk-test forwarding xdbg flags: " xdbg-flags)))
        (run-sequence-body env-extras parsed out-dir lenient?)))))

(def ^:private cli-table
  [{:cmds ["run"]
    :fn cmd-run
    :spec {:profile {:coerce :string
                     :default "stable"
                     :desc "stable | nightly"
                     :validate #{"stable" "nightly"}}
           :sample-size {:coerce :long
                         :desc "Number of nightly samples (default: 0 stable, 3 nightly)"
                         :validate nat-int?}}}
   {:cmds [] :fn (fn [_]
                   (println "Usage: cross-talk-test run [--profile stable|nightly]")
                   (println "                           [--sample-size N] [-- xdbg-flags...]"))}])

(defn- cli-error-fn [{:keys [msg]}]
  (d/eputs (str "cross-talk-test: " msg))
  (System/exit 2))

(defn -main [& args]
  (cli/dispatch cli-table args {:error-fn cli-error-fn}))

(when (= *file* (System/getProperty "babashka.file"))
  (apply -main *command-line-args*))
