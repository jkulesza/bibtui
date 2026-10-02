// Append inside src/app/tests.rs in the disposable review copy.
#[test]
fn review_perf_scaling() {
    use std::hint::black_box;
    use std::time::Instant;
    fn median_ms(mut f: impl FnMut(), count: usize) -> f64 {
        let mut times = Vec::new();
        for _ in 0..count {
            let t = Instant::now();
            f();
            times.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        times.sort_by(f64::total_cmp);
        times[count / 2]
    }
    for n in [1_000, 5_000, 10_000, 20_000] {
        let abstract_text =
            "A detailed discussion of reactor modelling and numerical methods. ".repeat(32);
        let mut input = String::new();
        for i in (0..n).rev() {
            input.push_str(&format!("@Misc{{K{i:06}, title={{Paper {i:06}}}, author={{Smith, John and Doe, Jane}}, year={{2020}}, abstract={{{abstract_text}}}}}\n"));
        }
        let parse_ms = median_ms(
            || {
                black_box(build_database(parse_bib_file(black_box(&input)).unwrap()));
            },
            3,
        );
        let (mut app, _dir) = review_app(&input);
        let entries: Vec<&Entry> = app.database.entries.values().collect();
        let mut engine = SearchEngine::new();
        let search_ms = median_ms(
            || {
                black_box(engine.search(&entries, black_box("reactor methods")));
            },
            5,
        );
        let author_ms = median_ms(
            || {
                black_box(engine.search(&entries, black_box("author:smith")));
            },
            5,
        );
        let cached: Vec<String> = entries
            .iter()
            .map(|e| crate::search::index::build_search_index(e))
            .collect();
        let pattern = nucleo_matcher::pattern::Pattern::new(
            "reactor methods",
            nucleo_matcher::pattern::CaseMatching::Ignore,
            nucleo_matcher::pattern::Normalization::Smart,
            nucleo_matcher::pattern::AtomKind::Fuzzy,
        );
        let mut matcher =
            nucleo_matcher::Matcher::new(nucleo_matcher::Config::DEFAULT.match_paths());
        let mut buf = Vec::new();
        let cached_match_ms = median_ms(
            || {
                for text in &cached {
                    black_box(
                        pattern.score(nucleo_matcher::Utf32Str::new(text, &mut buf), &mut matcher),
                    );
                }
            },
            5,
        );
        app.config.display.default_sort.field = "title".into();
        let sort_ms = median_ms(
            || {
                black_box(sort_entries(&app.database.entries, &app.config));
            },
            5,
        );
        let backend = ratatui::backend::TestBackend::new(120, 40);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let render_ms = median_ms(
            || {
                terminal.draw(|f| app.render(f)).unwrap();
            },
            5,
        );
        app.config
            .citekey
            .templates
            .insert("misc".into(), "R[title:camel]".into());
        let start = Instant::now();
        let renamed = app.regen_all_citekeys_impl(false);
        let regen_ms = start.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(renamed, n);
        println!("PERF n={n} bytes={} parse_ms={parse_ms:.3} search_ms={search_ms:.3} author_ms={author_ms:.3} cached_match_ms={cached_match_ms:.3} sort_ms={sort_ms:.3} render_ms={render_ms:.3} regen_ms={regen_ms:.3}", input.len());
    }
}
