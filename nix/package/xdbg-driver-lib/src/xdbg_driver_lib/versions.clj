(ns xdbg-driver-lib.versions
  (:require [xdbg-driver-lib.git :as git]
            [clojure.string :as str]))

(defn parse-stable-branch
  "Parse a branch name like \"v1.9\" or \"1.10.0\" into
   {:name :major :minor :patch} or nil if it doesn't match."
  [name]
  (when-let [[_ ver] (re-matches #"(v?\d+\.\d+(\.\d+)?)" name)]
    (let [parts (->> (str/split (str/replace ver #"^v" "") #"\.")
                     (map #(Long/parseLong %)))
          [maj min patch] (concat parts (repeat 0))]
      {:name name :major maj :minor min :patch (or patch 0)})))

(defn last-two-stable-branches
  "Return last two stable release branch names (newest first)."
  []
  (let [parsed (->> (git/for-each-ref "refs/remotes/origin/release/*")
                    (keep #(when-let [stripped (some-> %
                                                       (str/replace #"^origin/release/" ""))]
                             (parse-stable-branch stripped))))
        ;; dedupe by major.minor, keep highest patch
        deduped (->> parsed
                     (sort-by (juxt :major :minor :patch))
                     (reduce (fn [m {:keys [major minor] :as b}]
                               (assoc m [major minor] b))
                             {})
                     vals
                     (sort-by (juxt :major :minor :patch)))]
    (->> deduped
         (take-last 2)
         reverse
         (map :name))))

(defn nightly-tag-candidates
  "Return [[date sha7] ...] newest-first, deduped by (date, sha7).
   Tag format: <binding>-X.Y.Z-nightly.YYYYMMDD.<sha7>"
  []
  (let [re #".*-nightly\.(\d{8})\.([0-9a-f]{7})$"]
    (->> (git/tags-by-creator-date)
         (keep #(when-let [[_ date sha7] (re-matches re %)]
                  [date sha7]))
         distinct)))

(defn- semver-key
  "Sort key for a parsed branch: [kind-key padded-semver]."
  [{:keys [major minor patch]}]
  [0 (format "%010d.%010d.%010d" major minor patch)])

(defn- nightly-key
  "Sort key for a nightly: [1 YYYYMMDD]."
  [date]
  [1 date])

(defn- head-key [] [2 "0000000000.0000000000.0000000000"])

(defn pick-versions
  "Build the plan. Returns a vector of {:sha :short :role :branch :label}.
   :role is \"creator\" for the first required (non-nightly) entry,
   \"sender\" for the rest."
  [{:keys [sample-size] :or {sample-size 3}}]
  (let [branches (last-two-stable-branches)
        _ (when (< (count branches) 2)
            (throw (ex-info "need at least 2 stable release branches"
                            {:found (count branches)})))
        [newest next-b] branches
        [newest-sha next-sha head-sha]
        (git/rev-parse-multi (str "origin/release/" newest)
                             (str "origin/release/" next-b)
                             "HEAD")
        head-ref (git/abbrev-ref-head)
        head-label (if head-ref (str "HEAD@" head-ref) "HEAD")
        base [{:sha newest-sha
               :branch (str "release/" newest)
               :label ""
               :sort-key (semver-key (parse-stable-branch newest))}
              {:sha next-sha
               :branch (str "release/" next-b)
               :label ""
               :sort-key (semver-key (parse-stable-branch next-b))}
              {:sha head-sha
               :branch ""
               :label head-label
               :sort-key (head-key)}]
        nightlies (when (pos? sample-size)
                    (for [[date sha7] (take sample-size (nightly-tag-candidates))
                          :let [full (git/rev-parse-verify (str sha7 "^{commit}"))]
                          :when full]
                      {:sha full
                       :branch ""
                       :label (str "nightly." date)
                       :sort-key (nightly-key date)}))
        ;; Dedup by sha. base is first so it wins over nightly dupes.
        all (->> (concat base nightlies)
                 (reduce (fn [{:keys [seen out]} entry]
                           (if (contains? seen (:sha entry))
                             {:seen seen :out out}
                             {:seen (conj seen (:sha entry))
                              :out (conj out entry)}))
                         {:seen #{} :out []})
                 :out)
        sorted (sort-by :sort-key all)
        ;; Assign role: first required (label="") is creator, others sender.
        with-role (loop [[e & rest] sorted
                         creator-seen? false
                         acc []]
                    (if-not e
                      acc
                      (let [required? (str/blank? (:label e))
                            role (if (and (not creator-seen?) required?)
                                   "creator"
                                   "sender")]
                        (recur rest
                               (or creator-seen? (= role "creator"))
                               (conj acc (-> e
                                             (assoc :role role
                                                    :short (subs (:sha e) 0 7))
                                             (dissoc :sort-key)))))))]
    (vec with-role)))
