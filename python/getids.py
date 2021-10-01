import gspread
import json
from oauth2client.service_account import ServiceAccountCredentials

scope = ['https://spreadsheets.google.com/feeds', 'https://www.googleapis.com/auth/drive']
credentials = ServiceAccountCredentials.from_json_keyfile_name('eft-sheet-updater-96373fea2213.json', scope)
gc = gspread.authorize(credentials)
spreadsheet = gc.open_by_key('1YVWW8bBsGZthcil8HRrrt0uwYO6h5YXVGaC_z0Ety4A')
api_ref = spreadsheet.get_worksheet(3)
sheet_ids = api_ref.col_values(3) # All values from ID column (3)

ids = {}
ids["ids"] = []

index = 0
for id in sheet_ids:
    if index != 0:
        ids["ids"].append(id)
    else:
        index += 1 # Skip title row

with open('item_ids.json', 'w') as outfile:
    json.dump(ids, outfile)