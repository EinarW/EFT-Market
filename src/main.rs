use tarkov::auth::LoginError;
use tarkov::hwid::generate_hwid;
use tarkov::{Error, Tarkov};
use std::io;
use std::collections::HashMap;
use tarkov::market_filter::{MarketFilter, Owner, SortDirection};
use tarkov::profile::Side;
use std::fs;
use std::fs::File;
use std::io::Write;
use serde_json;
use indicatif::ProgressBar;
use std::process::Command;
extern crate ajson;

#[tokio::main]
async fn main() -> Result<(), Error> {
    std::env::set_var("RUST_LOG", "tarkov=info");
    
    println!("\nAuthenticating with Tarkov API...");

    let debug = false;
    let email = "";
    let password = "";
    let mut hwid = fs::read_to_string("hwid.txt")
                .expect("Something went wrong reading hwid!");

    if hwid == "" {
        // Create a new hwid, will require 2fa
        hwid = generate_hwid();
        let mut file_hwid = File::create("hwid.txt")?;
        let byte_hwid = hwid.as_bytes();
        file_hwid.write_all(byte_hwid)?;
    }

    let t = match Tarkov::login(email, password, &hwid).await {
        Ok(t) => Ok(t),
        Err(Error::LoginError(e)) => match e {
            // 2FA required!
            LoginError::TwoFactorRequired => {

                // Get 2FA from email (or generate TOTP) then continue...
				let mut code = String::new();
				io::stdin().read_line(&mut code).expect("Failed to read input");
				let len = code.trim_end_matches(&['\r', '\n'][..]).len();
				code.truncate(len);
                Tarkov::login_with_2fa(email, password, &code, &hwid).await
            }
            _ => Err(e)?,
        },
        Err(e) => Err(e),
    }?;

    
    println!("Authenticated!");
    println!("\nSelecting PMC profile...");


    // Find and select PMC profile to complete login.
    let sess = Tarkov::from_session(&t.session);
    let profiles = sess.get_profiles().await?;
    let profile = profiles
        .into_iter()
        .find(|p| p.info.side != Side::Savage)
        .unwrap();
    sess.select_profile(&profile.id).await?;
    

    println!("Profile selected!");
    println!("\nFetching items from spreadsheet...");


    /* Fetch items */
    let py_fetch_items = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(&["/C", "python python\\getids.py"])
            .output()
            .expect("failed to execute process")
    } else {
        Command::new("sh")
            .args(&["-c", "python3 python/getids.py"])
            .output()
            .expect("failed to execute process")
    };
    py_fetch_items.stdout;
    let items_file = File::open("item_ids.json").unwrap();
    let items = ajson::parse_from_read(items_file).unwrap();
    let items_length = items.get("ids.#").unwrap().to_u64();


    println!("Fetched items!");
    println!("\nFetching item base prices...");


    /* Get all item base prices */
    let item_base_prices_result = sess.get_item_prices().await?;
    let file_base_prices = File::create("prices.json")?;
    serde_json::to_writer_pretty(file_base_prices, &item_base_prices_result)?;
    

    println!("Fetched base prices!");
    println!("\nFetching number of offers...");


    /* Fetch total number of offers per item */
    let offer_categories_result = sess.search_market(
        0,
        1,
        MarketFilter {
            min_quantity: Some(1),
            min_condition: Some(80),
            max_condition: Some(100),
            sort_direction: SortDirection::Ascending,
            owner_type: Owner::Player, // Traders, Player, Any
            hide_bartering_offers: true,
            hide_inoperable_weapons: true,
            ..MarketFilter::default()
        }
    ).await?;

    let offer_categories = offer_categories_result.categories;


    println!("Fetched number of offers!");
    println!("\nProcessing market data (estimated time 30 seconds):\n");

    /* Everything else */
    // Create HashMap to hold averages
    let mut map_averages: HashMap<String, u64> = HashMap::new();

    // Create progressbar
    let pb = ProgressBar::new(items_length);

    // Process each item
    for i in 0..items_length {
        let item = &items.get(&format!("ids.{}", i)).unwrap().as_str().to_string();

        // Set item as Option<String>
        let item_id: Option<String> = {
            let id = Some(item.to_owned());
            id
        };
        
        // Set number of offers to fetch
        let found_total_offers =  match offer_categories.get(item.as_str()) {
            None => 0 as u64,
            Some(x) => *x
        };

        let offers_min_limit = 20;
        let mut offers_to_find = offers_min_limit;
        if found_total_offers > offers_min_limit * 8 {
            offers_to_find = (found_total_offers as f64 * 0.075) as u64;
        } else if found_total_offers > offers_min_limit * 4 {
            offers_to_find = (found_total_offers as f64 * 0.15) as u64;
        } else if found_total_offers > offers_min_limit * 2 {
            offers_to_find = (found_total_offers as f64 * 0.3) as u64;
        }

        
        if offers_to_find != 0 {

            // Get market search results by item
            let result = sess.search_market(
                0, 
                offers_to_find,
                MarketFilter {
                    min_quantity: Some(1),
                    min_condition: Some(90),
                    max_condition: Some(100),
                    sort_direction: SortDirection::Ascending,
                    owner_type: Owner::Any, // Traders, Player, Any
                    hide_bartering_offers: true,
                    hide_inoperable_weapons: true,
                    handbook_id: item_id.as_deref(),
                    ..MarketFilter::default()
                }
            ).await?;
            let offers_count = result.offers_count;
            let offers = result.offers;
            // Same check as "if offers_to_find == 0" above, but in case offers have changed, we check again
            if offers_count != 0 {
                let mut price_weight: std::collections::HashMap<u64, f64> = HashMap::new();
                let mut offers_shifted_index = 1;

                for offer in offers {
                    // Get offer price
                    let offer_price = offer.requirements_cost;  

                    // Get offer object count
                    let offer_item = &offer.items[0];
                    let offer_upd = match &offer_item.upd {
                        Some(x) => x,
                        None => &tarkov::inventory::Upd {
                            stack_objects_count:None,
                            spawned_in_session:None,
                            med_kit:None,
                            repairable:None,
                            light:None,
                            unlimited_count:None,
                            buy_restriction_max:None,
                            buy_restriction_current:None,
                            key:None
                        }
                    };
                    let mut offer_o_count = match &offer_upd.stack_objects_count {
                        None => 0 as u64,
                        Some(x) => *x
                    };

                    // Get member type
                    let offer_member_type: u64 = offer.user.member_type;


                    // Limit weight of large offers
                    if offer_o_count > 100 {
                        if offer_member_type == 4 {
                            if offer_o_count > 300 {
                                offer_o_count = 300;
                            }
                        } else {
                            offer_o_count = 100;
                        }
                    }

                    // Current offer is what percent of total offers?
                    let offer_position = ((offers_shifted_index as f64 / offers_to_find as f64) * 100.0) as u64;

                    // Set offer weight
                    let offer_weight: f64;
                    if offers_count > offers_min_limit {
                        if offer_position >= 80 {
                            offer_weight = offer_o_count as f64 * 0.1;
                        } else if offer_position >= 60 {
                            offer_weight = offer_o_count as f64 * 0.5;
                        } else if offer_position >= 20 {
                            offer_weight = offer_o_count as f64 * 1.0;
                        } else {
                            offer_weight = offer_o_count as f64 * 0.75;
                        }
                    } else {
                        if offer_position >= 60 {
                            offer_weight = offer_o_count as f64 * 0.15;
                        } else if offer_position >= 30 {
                            offer_weight = offer_o_count as f64 * 1.0;
                        } else {
                            offer_weight = offer_o_count as f64 * 0.75;
                        }
                    }


                    // Weighted price
                    let offer_stack_price = (offer_price as f64 * offer_weight) as u64;

                    // Add price and weight to HashMap
                    price_weight.insert(offer_stack_price, offer_weight);
                    offers_shifted_index += 1;
                }

                // Get item weighted average
                let mut offer_weighted_price: f64 = 0.0;
                let mut offer_total_weight: f64 = 0.0;
                for (offer_k, offer_v) in price_weight {
                    offer_weighted_price += offer_k as f64;
                    offer_total_weight += offer_v;
                }

                let mut offer_weighted_average = (offer_weighted_price / offer_total_weight) as u64;

                // Correct items skewed because of bugged filtering on condition 100/100
                if item == "5d1b36a186f7742523398433" {
                    offer_weighted_average = (offer_weighted_average as f64 * 5.75) as u64;
                }


                // Add item and its average to the map
                map_averages.insert(item.to_string(), offer_weighted_average);
            }
        }
        if !debug {
            pb.inc(1);
        }
    }
    pb.finish_with_message("Market data processed!");
    println!("\nUpdating price history...");





    let pb2 = ProgressBar::new(items_length);
    // Get the price history index
    let index_file = File::open("./price_history/index.json").unwrap();
    let index_data = ajson::parse_from_read(index_file).unwrap();
    let mut index = index_data.get("index").unwrap().to_u64();

    if index > 143 {  // For 10 min interval: (24 * 60) / 10 = 144
        index  = 0;
    }

    // Write the history average
    let file_averages = File::create(format!("./price_history/averages_{}.{}", index, "json"))?;
    serde_json::to_writer_pretty(file_averages, &map_averages)?;



    // Get average history per item, and take the total average
    let mut map_real_averages: HashMap<String, u64> = HashMap::new();

    for i in 0..items_length {
        let item = &items.get(&format!("ids.{}", i)).unwrap().as_str().to_string();

        let mut dividend = 0;
        let mut total: f64 = 0.0;

        let path_list_of_averages = fs::read_dir("./price_history").unwrap();

        for path in path_list_of_averages {
            let file_path = path.unwrap().path().display().to_string();
            let file_name = &file_path[16..]; // Remove first path from string

            if  file_name != "index.json" {
                dividend += 1;

                let hist_avg_file = File::open(format!("./price_history/{}", &file_name)).unwrap();
                let hist_avg = ajson::parse_from_read(hist_avg_file).unwrap();
                let avg = hist_avg.get(&item).unwrap().to_f64();

                total += avg;                
            }
        }

        let real_avg = (total as u64 / dividend) as u64;
        map_real_averages.insert(item.to_string(), real_avg);

        if !debug {
            pb2.inc(1);
        }
    }    

    // Write real averages to file
    let file_averages = File::create("averages.json")?;
    serde_json::to_writer_pretty(file_averages, &map_real_averages)?;

    // Update index
    let mut new_index: HashMap<String, u64> = HashMap::new();
    new_index.insert("index".to_string(), &index + 1);
    let new_index_file = File::create("price_history/index.json")?;
    serde_json::to_writer_pretty(new_index_file, &new_index)?;

    pb2.finish_with_message("Price history updated!");
    



    
    // Run python script updating excel spreadsheet
    println!("\nPushing changes...");

    let py_push = if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(&["/C", "python python\\main.py"])
            .output()
            .expect("failed to execute process")
    } else {
        Command::new("sh")
            .args(&["-c", "python3 python/main.py"])
            .output()
            .expect("failed to execute process")
    };
    py_push.stdout;

    println!("Done!\n");

    Ok(())
}