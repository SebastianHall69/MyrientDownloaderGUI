use iced::futures::SinkExt;
use iced::futures::channel::mpsc::Sender;
use iced::widget::Column;
use iced::{Task, widget};
use regex::Regex;
use scraper::{Html, Selector};
use std::fmt::{Display, Formatter};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Debug)]
enum Message {
    PageUrlChange(String),
    SearchForDownloadsPressed,
    DownloadLinksFound(Vec<RemoteFileEntry>),
    FileSelected(usize, bool),
    StartDownloads,
    DownloadProgressUpdated(f32, String),
    SingleDownloadFinished,
    AllDownloadsFinished,
    DownloadStatusToggleFlipped(bool),
    RegexStatusToggleFlipped(bool),
    IncludeRegexContentChanged(widget::text_editor::Action),
    ExcludeRegexContentChanged(widget::text_editor::Action),
    DownloadDirectoryPressed,
    DownloadDirectoryUpdated(PathBuf),
}

#[derive(Clone, Debug, Hash)]
struct RemoteFile {
    link: String,
    title: String,
}

#[derive(Clone, Debug)]
struct RemoteFileEntry {
    file: RemoteFile,
    should_download: bool,
}

impl Display for RemoteFileEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.file.title)
    }
}

struct Model {
    page_url: String,
    file_entries: Vec<RemoteFileEntry>,
    include_regex_content: widget::text_editor::Content,
    exclude_regex_content: widget::text_editor::Content,
    is_all_selected: bool,
    is_regex_case_sensitive: bool,
    download_directory: PathBuf,
    num_files_to_download: usize,
    num_files_downloaded: usize,
    single_file_download_progress: f32,
    is_downloading: bool,
    is_searching_for_links: bool,
    progress_bar_title: String,
}

impl Default for Model {
    fn default() -> Self {
        let include_regex_content: widget::text_editor::Content = widget::text_editor::Content::with_text(
            "\
            \\([^)]*usa*[^)]\\)\
            ",
        );
        let exclude_regex_content: widget::text_editor::Content = widget::text_editor::Content::with_text(
            "\
            \\([^)]*demo*[^)]\\)\n\
            \\([^)]*beta*[^)]\\)\
            ",
        );
        let is_all_selected = true;
        let file_entries = vec![RemoteFileEntry {
            should_download: is_all_selected,
            file: RemoteFile {
                link: String::new(),
                title: String::from("No downloads found"),
            },
        }];
        let num_files_to_download = file_entries.len();

        Self {
            page_url: String::from("https://myrient.erista.me/files/Redump/Sony%20-%20PlayStation/"),
            file_entries,
            include_regex_content,
            exclude_regex_content,
            is_all_selected,
            is_regex_case_sensitive: false,
            download_directory: std::env::current_dir().expect("Could not read current directory"),
            num_files_to_download,
            num_files_downloaded: 0,
            single_file_download_progress: 0.0,
            is_downloading: false,
            is_searching_for_links: false,
            progress_bar_title: String::from("No File"),
        }
    }
}

impl Model {
    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::PageUrlChange(text) => {
                self.page_url = text;
                Task::none()
            }
            Message::SearchForDownloadsPressed => {
                self.is_searching_for_links = true;
                Task::perform(
                    get_downloadable_links(
                        self.page_url.clone(),
                        self.get_inclusion_regex(),
                        self.get_exclusion_regex(),
                        self.is_all_selected,
                    ),
                    Message::DownloadLinksFound,
                )
            }
            Message::DownloadLinksFound(file_entries) => {
                self.file_entries = file_entries;
                self.num_files_to_download = self.file_entries.len();
                self.num_files_downloaded = 0;
                self.is_searching_for_links = false;
                Task::none()
            }
            Message::FileSelected(index, is_selected) => {
                if let Some(entry) = self.file_entries.get_mut(index) {
                    entry.should_download = is_selected;
                    if entry.should_download {
                        self.num_files_to_download += 1;
                    } else {
                        self.num_files_to_download -= 1;
                    }
                }
                Task::none()
            }
            Message::StartDownloads => {
                self.is_downloading = true;
                self.num_files_downloaded = 0;
                Task::none()
            }
            Message::AllDownloadsFinished => {
                self.is_downloading = false;
                Task::none()
            }
            Message::DownloadStatusToggleFlipped(status) => {
                self.is_all_selected = status;

                if self.is_all_selected {
                    self.num_files_to_download = self.file_entries.len();
                } else {
                    self.num_files_to_download = 0;
                }

                for file_entry in &mut self.file_entries {
                    file_entry.should_download = self.is_all_selected;
                }
                Task::none()
            }
            Message::IncludeRegexContentChanged(action) => {
                self.include_regex_content.perform(action);
                Task::none()
            }
            Message::ExcludeRegexContentChanged(action) => {
                self.exclude_regex_content.perform(action);
                Task::none()
            }
            Message::RegexStatusToggleFlipped(flag) => {
                self.is_regex_case_sensitive = flag;
                Task::none()
            }
            Message::DownloadDirectoryPressed => Task::perform(get_directory(), Message::DownloadDirectoryUpdated),
            Message::DownloadDirectoryUpdated(dir) => {
                self.download_directory = dir;
                Task::none()
            }
            Message::SingleDownloadFinished => {
                self.num_files_downloaded += 1;
                self.single_file_download_progress = 0.0;
                Task::none()
            }
            Message::DownloadProgressUpdated(percentage, title) => {
                self.single_file_download_progress = percentage;
                self.progress_bar_title = title;
                Task::none()
            }
        }
    }

    fn view(&self) -> Column<'_, Message> {
        let mut file_list = Column::new().spacing(10);

        for (index, entry) in self.file_entries.iter().enumerate() {
            let mut check_box = widget::checkbox(entry.should_download).label(&entry.file.title);
            if !self.is_downloading && !self.is_searching_for_links {
                check_box = check_box.on_toggle(move |is_selected| Message::FileSelected(index, is_selected));
            }

            file_list = file_list.push(check_box);
        }

        let mut page_url_text_input = widget::text_input("https://myrient.erista.me/files/", &self.page_url);
        let mut select_all_toggle = widget::toggler(self.is_all_selected);
        let mut regex_case_insensitive_toggle = widget::toggler(self.is_regex_case_sensitive);
        let mut download_dir_button = widget::button("Download Directory");
        let mut search_for_downloads_button = widget::button(widget::text("Search For Downloads").center().width(iced::Length::Fill));
        let mut include_regex_text_editor = widget::text_editor(&self.include_regex_content).min_height(60);
        let mut exclude_regex_text_editor = widget::text_editor(&self.exclude_regex_content).min_height(60);
        let mut start_download_button =
            widget::button(widget::text("Start Downloads").center().width(iced::Length::Fill)).width(iced::Length::Fill);

        if !self.is_downloading && !self.is_searching_for_links {
            page_url_text_input = page_url_text_input.on_input(Message::PageUrlChange);
            select_all_toggle = select_all_toggle.on_toggle(Message::DownloadStatusToggleFlipped);
            regex_case_insensitive_toggle = regex_case_insensitive_toggle.on_toggle(Message::RegexStatusToggleFlipped);
            download_dir_button = download_dir_button.on_press(Message::DownloadDirectoryPressed);
            search_for_downloads_button = search_for_downloads_button.on_press(Message::SearchForDownloadsPressed);
            include_regex_text_editor = include_regex_text_editor.on_action(Message::IncludeRegexContentChanged);
            exclude_regex_text_editor = exclude_regex_text_editor.on_action(Message::ExcludeRegexContentChanged);
            start_download_button = start_download_button.on_press(Message::StartDownloads);
        }

        widget::column![
            widget::row![widget::text("Myrient Page Link"), page_url_text_input]
                .spacing(5)
                .align_y(iced::alignment::Vertical::Center),
            widget::row![
                widget::row![select_all_toggle, widget::text("Select All"),]
                    .padding(5)
                    .spacing(5)
                    .align_y(iced::alignment::Vertical::Center),
                widget::row![regex_case_insensitive_toggle, widget::text("Case Sensitive Regex"),]
                    .padding(5)
                    .spacing(5)
                    .align_y(iced::alignment::Vertical::Center),
                widget::row![
                    download_dir_button,
                    widget::text(self.download_directory.to_str().unwrap_or("Pick A Path Bozo"))
                ]
                .padding(5)
                .spacing(5)
                .align_y(iced::alignment::Vertical::Center)
            ]
            .spacing(20)
            .align_y(iced::alignment::Vertical::Center),
            search_for_downloads_button,
            widget::row![
                widget::scrollable(file_list).height(iced::Length::Fill).width(iced::Length::Fill),
                widget::column![
                    widget::column![widget::text("Include filters"), widget::scrollable(include_regex_text_editor)].spacing(5),
                    widget::column![widget::text("Exclude filters"), widget::scrollable(exclude_regex_text_editor)].spacing(5),
                    widget::column![
                        widget::text(format!(
                            "Files Complete: {} / {}",
                            self.num_files_downloaded, self.num_files_to_download
                        )),
                        widget::progress_bar(
                            0.0f32..=100.0f32,
                            self.num_files_downloaded as f32 / (self.num_files_to_download as f32 + f32::EPSILON) * 100.0
                        )
                    ]
                    .spacing(5),
                    widget::column![
                        widget::text(format!("Download Progress: {}", self.progress_bar_title)),
                        widget::progress_bar(0.0..=100.0, self.single_file_download_progress)
                    ]
                    .spacing(5)
                ]
                .padding(20)
                .spacing(20)
            ],
            start_download_button
        ]
        .spacing(20)
        .padding(20)
    }

    fn download_files(&self) -> iced::Subscription<Message> {
        if self.is_downloading {
            let files_to_download: Vec<RemoteFile> = self
                .file_entries
                .iter()
                .filter_map(|file_entry| match file_entry.should_download {
                    true => Some(file_entry.file.clone()),
                    false => None,
                })
                .collect();

            iced::Subscription::run_with(
                (files_to_download, self.download_directory.clone()),
                move |(files_to_download, download_directory)| {
                    let files = files_to_download.clone();
                    let dir = download_directory.clone();

                    iced::stream::channel(100, move |mut sender: Sender<Message>| async move {
                        for file in files {
                            println!("Downloading: {}", &file.title);
                            let response = reqwest::get(&file.link).await;

                            match response {
                                Ok(mut resp) => {
                                    let total_size = resp.content_length().unwrap_or(0) as f32;
                                    let mut size_downloaded = 0.0f32;

                                    let path = dir.join(&file.title);
                                    if let Some(parent) = path.parent() {
                                        std::fs::create_dir_all(parent).expect("You don't have permission to create parent directories");
                                    }

                                    let mut file_path = OpenOptions::new()
                                        .create(true)
                                        .write(true)
                                        .truncate(true)
                                        .open(&path)
                                        .expect("You don't have permission to open the file");

                                    while let Ok(Some(chunk)) = resp.chunk().await {
                                        file_path.write_all(&chunk).expect("Failed to write chunk");
                                        size_downloaded += chunk.len() as f32;
                                        let progress = size_downloaded / total_size * 100.0;
                                        let _ = sender.send(Message::DownloadProgressUpdated(progress, file.title.clone())).await;
                                    }
                                }
                                Err(err) => {
                                    println!("Failed to start download for: {:?}. ERR: {}", err.url(), err);
                                }
                            }

                            let _ = sender.send(Message::SingleDownloadFinished).await;
                        }

                        let _ = sender.send(Message::AllDownloadsFinished).await;
                        iced::futures::pending!()
                    })
                },
            )
        } else {
            iced::Subscription::none()
        }
    }

    fn get_inclusion_regex(&self) -> Vec<Regex> {
        self.parse_regex(&self.include_regex_content)
    }

    fn get_exclusion_regex(&self) -> Vec<Regex> {
        self.parse_regex(&self.exclude_regex_content)
    }

    fn parse_regex(&self, content: &widget::text_editor::Content) -> Vec<Regex> {
        content
            .lines()
            .map(|line| {
                if self.is_regex_case_sensitive {
                    line.text.to_string()
                } else {
                    format!("(?i){}", line.text)
                }
            })
            .filter_map(|str| Regex::new(str.as_str()).ok())
            .collect()
    }
}

fn main() -> iced::Result {
    iced::application(|| (Model::default(), Task::none()), Model::update, Model::view)
        .subscription(Model::download_files)
        .title("Based Myrient Downloader")
        .run()
}

async fn get_directory() -> PathBuf {
    rfd::AsyncFileDialog::new()
        .set_title("Download Directory")
        .pick_folder()
        .await
        .map_or(PathBuf::new(), |handle| handle.path().to_path_buf())
}

async fn get_downloadable_links(
    url: String,
    include_regex: Vec<Regex>,
    exclude_regex: Vec<Regex>,
    is_all_selected: bool,
) -> Vec<RemoteFileEntry> {
    let page_text = reqwest::get(&url)
        .await
        .expect("Failed to find page")
        .text()
        .await
        .expect("Failed to retrieve page text");

    let parser = Html::parse_document(page_text.as_str());
    let link_selector = Selector::parse("td.link a").expect("Wow you are really bad at parsing html");

    parser
        .select(&link_selector)
        .filter(|link_element| link_element.attr("title").is_some() && link_element.attr("href").is_some())
        .map(|link_element| RemoteFile {
            title: link_element.attr("title").expect("Failed to get title").to_string(),
            link: format!("{}{}", &url, link_element.attr("href").expect("Failed to get download link")),
        })
        .filter(|file| include_regex.iter().all(|regex| regex.is_match(file.title.as_str())))
        .filter(|file| !exclude_regex.iter().any(|regex| regex.is_match(file.title.as_str())))
        .map(|file| RemoteFileEntry {
            file,
            should_download: is_all_selected,
        })
        .collect()
}

// Add recursive
// Add success log to skip successful downloads (by download url)
